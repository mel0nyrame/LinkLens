//! AI 出口检测编排：Claude/GPT 两页同构探测，一框架参数化复用。
//!
//! 每轮三出口并行（国内出口复用首页的双源采集流程、
//! Cloudflare 出口 trace 1.1.1.1、AI 出口 trace 平台域名）→ 出口 IP 拉
//! iprisk + geoip（IPv6 无 geo 时用 trace `loc=` 兜底）→ 可用性探测分档 →
//! 服务状态 → 历史记录（同 IP 24h 去重，落用户历史目录）。
//!
//! 公网探测采用客户端级 8 秒边界；AI 风险聚合请求单独设为 10 秒。

use crate::app::Page;
use crate::detect;
use crate::history;
use crate::net::{geoip, http, iprisk, latency::LatencyProbe, status, trace};
use crate::state::SharedState;
use crate::state_ai::{AiOutcome, AiPhase};

/// 一个 AI 检测页的全部参数：GPT 页与 Claude 页同构，仅以下取值不同。
pub struct AiProfile {
    /// 归属页面。
    pub page: Page,
    /// 品牌名（卡片标题用）。
    pub brand: &'static str,
    /// AI 出口 trace 域名。
    pub trace_host: &'static str,
    /// 服务状态路径（相对 `net::geoip::API_BASE`）。
    pub status_path: &'static str,
    /// 可用性探测目标 `（名称, URL）`。
    pub availability_targets: &'static [(&'static str, &'static str)],
    /// 命中受限地区时的红色警示副文案。
    pub restricted_hint: &'static str,
    /// 历史文件标签（`<用户历史目录>/{tag}-history.json`）。
    pub history_tag: &'static str,
}

/// Claude 检测页参数（报告 §3.2）。
pub const CLAUDE_PROFILE: AiProfile = AiProfile {
    page: Page::Claude,
    brand: "Claude",
    trace_host: "claude.ai",
    status_path: "claude/status.json",
    availability_targets: &[
        ("Claude", "https://claude.ai/cdn-cgi/trace"),
        ("Anthropic", "https://www.anthropic.com/cdn-cgi/trace"),
    ],
    restricted_hint: "不建议尝试登录 Claude",
    history_tag: "claude",
};

/// GPT 检测页参数（报告 §3.4：与 Claude 完全同构，仅出口源/探测目标/状态路径不同）。
pub const GPT_PROFILE: AiProfile = AiProfile {
    page: Page::Gpt,
    brand: "GPT",
    trace_host: "chatgpt.com",
    status_path: "gpt/status.json",
    availability_targets: &[
        ("ChatGPT", "https://chatgpt.com/cdn-cgi/trace"),
        ("OpenAI API", "https://api.openai.com/"),
    ],
    restricted_hint: "不建议尝试登录 ChatGPT",
    history_tag: "gpt",
};

/// 页面到参数的映射；非 AI 页为 `None`。
pub fn profile_for(page: Page) -> Option<&'static AiProfile> {
    match page {
        Page::Claude => Some(&CLAUDE_PROFILE),
        Page::Gpt => Some(&GPT_PROFILE),
        _ => None,
    }
}

/// 事件循环每次迭代调用：当前页是 AI 页且尚未启动时启动探测（幂等）。
pub fn spawn_for_page_if_needed(shared: SharedState) {
    let should_spawn = {
        let mut state = shared.lock();
        match profile_for(state.app.page) {
            Some(profile) => {
                let page_state = state
                    .ai
                    .page_mut(profile.page)
                    .expect("profile 的页面必然在 AiState 中");
                if page_state.started {
                    None
                } else {
                    page_state.begin_probe();
                    Some(profile)
                }
            }
            None => None,
        }
    };
    if let Some(profile) = should_spawn {
        spawn_probe(shared, profile);
    }
}

/// `r` 键触发的重查：复位页面状态后重新探测；探测进行中忽略。
pub fn request_refresh(shared: &SharedState, page: Page) {
    let Some(profile) = profile_for(page) else {
        return;
    };
    {
        let mut state = shared.lock();
        let Some(page_state) = state.ai.page_mut(profile.page) else {
            return;
        };
        if !page_state.begin_probe() {
            return;
        }
    }
    spawn_probe(shared.clone(), profile);
}

/// 调用方已在状态锁内认领 Pending，再启动 IO 任务。
fn spawn_probe(shared: SharedState, profile: &'static AiProfile) {
    shared.notify();
    let client = http::client();
    let probe = LatencyProbe::new();
    tokio::spawn(async move {
        run_page(&client, &probe, shared, profile).await;
    });
}

/// 单页完整探测：本轮三出口并行 → AI 出口 iprisk/geoip/可用性/服务状态并行 →
/// 受限判定 → 历史记录 → 写回状态。
async fn run_page(
    client: &reqwest::Client,
    probe: &LatencyProbe,
    shared: SharedState,
    profile: &'static AiProfile,
) {
    // 读回持久化历史（重启后可查）；Pending 已由启动方在同锁内设定。
    let loaded_history = {
        let mut state = shared.lock();
        let path = history::history_path(profile.history_tag);
        let loaded = path.as_ref().map(|p| history::load(p)).unwrap_or_default();
        let page_state = state
            .ai
            .page_mut(profile.page)
            .expect("profile 的页面必然在 AiState 中");
        page_state.history = loaded.clone();
        drop(state);
        shared.notify();
        loaded
    };

    // 国内/Cloudflare 与平台出口并行采集，结果只属于当前 AI 页的本轮快照。
    let (reference_egress, (exit, risk_geo, availability, service_status)) =
        tokio::join!(crate::probe::fetch_egress_cards(client), async {
            let exit = trace::fetch_trace(client, profile.trace_host).await;
            let exit_ip = exit.as_ref().and_then(|t| t.ip.as_deref());
            let (risk_geo, availability, service_status) = tokio::join!(
                fetch_risk_and_geo(client, exit_ip, exit.as_ref()),
                probe_availability(probe, profile),
                status::fetch_status(client, profile.status_path),
            );
            (exit, risk_geo, availability, service_status)
        });
    let exit_ip = exit.as_ref().and_then(|t| t.ip.as_deref());
    let exit_trace = exit.as_ref();
    let (risk, geo, geo_from_trace) = risk_geo;

    // 受限判定：出口国别码（geoip 优先，trace loc 兜底）命中硬表
    let restricted = {
        let country = detect::ai::resolve_exit_country(
            geo.as_ref(),
            exit_trace.and_then(|t| t.loc.as_deref()),
        );
        country
            .as_ref()
            .map(|c| detect::ai::is_restricted(&c.code))
            .unwrap_or(false)
    };

    // 历史记录：有效分值才能新增；缺分不占用同 IP 的 24h 去重窗口。
    if let Some(ip) = exit_ip {
        let trust = risk.as_ref().and_then(|r| r.trust_score);
        let now = history::now_ms();
        let path = history::history_path(profile.history_tag);
        let entries = history::record(&loaded_history, ip, trust, restricted, now);
        if trust.is_some()
            && let Ok(path) = path
        {
            let _ = history::save(&path, &entries);
        }
        let mut state = shared.lock();
        if let Some(page_state) = state.ai.page_mut(profile.page) {
            page_state.history = entries;
        }
    }

    {
        let mut state = shared.lock();
        if let Some(page_state) = state.ai.page_mut(profile.page) {
            page_state.phase = AiPhase::Done(Box::new(AiOutcome {
                reference_egress,
                exit,
                risk,
                geo,
                geo_from_trace,
                restricted,
                availability,
                status: service_status,
            }));
        }
    }
    shared.notify();
}

/// 出口 IP 的 iprisk + geoip 并行拉取；geoip 拿不到国别码且 trace 有 `loc=` 时标记兜底。
async fn fetch_risk_and_geo(
    client: &reqwest::Client,
    exit_ip: Option<&str>,
    exit_trace: Option<&trace::Trace>,
) -> (Option<iprisk::Iprisk>, Option<geoip::GeoIp>, bool) {
    let Some(ip) = exit_ip else {
        return (None, None, false);
    };
    let (risk, geo) = tokio::join!(
        iprisk::fetch_iprisk(client, ip),
        geoip::fetch_geoip(client, ip)
    );
    let geo_cc_missing = geo
        .as_ref()
        .map(|g| g.country_code.trim().is_empty())
        .unwrap_or(true);
    let has_trace_loc = exit_trace
        .and_then(|t| t.loc.as_deref())
        .is_some_and(|loc| !loc.trim().is_empty());
    (risk, geo, geo_cc_missing && has_trace_loc)
}

/// 可用性探测：逐目标一次 TLS 握手计时（8s 红线内），全目标并行。
async fn probe_availability(
    probe: &LatencyProbe,
    profile: &'static AiProfile,
) -> Vec<(&'static str, Option<u64>)> {
    let measures = profile
        .availability_targets
        .iter()
        .map(|(name, url)| async move { (*name, probe.measure_url(url).await) });
    futures_util::future::join_all(measures).await
}

#[cfg(test)]
mod tests {
    use super::{CLAUDE_PROFILE, GPT_PROFILE, profile_for};
    use crate::app::Page;
    use crate::history;

    #[test]
    fn claude_and_gpt_profiles_differ_only_on_official_axes() {
        // 报告 §3.4：同构框架，仅出口源/探测目标/状态路径三处不同
        assert_eq!(CLAUDE_PROFILE.trace_host, "claude.ai");
        assert_eq!(GPT_PROFILE.trace_host, "chatgpt.com");
        assert_eq!(CLAUDE_PROFILE.status_path, "claude/status.json");
        assert_eq!(GPT_PROFILE.status_path, "gpt/status.json");
        assert_eq!(CLAUDE_PROFILE.brand, "Claude");
        assert_eq!(GPT_PROFILE.brand, "GPT");
    }

    #[test]
    fn availability_targets_match_site() {
        assert!(
            CLAUDE_PROFILE
                .availability_targets
                .iter()
                .any(|(_, url)| url.contains("anthropic.com"))
        );
        // GPT 探测目标含 api.openai.com（Codex/API 链路提示）
        assert!(
            GPT_PROFILE
                .availability_targets
                .iter()
                .any(|(_, url)| url.contains("api.openai.com"))
        );
    }

    #[test]
    fn history_paths_share_the_user_data_directory() {
        for tag in [CLAUDE_PROFILE.history_tag, GPT_PROFILE.history_tag] {
            let path = history::history_path(tag).unwrap();
            let file = path
                .file_name()
                .expect("历史路径应有文件名")
                .to_string_lossy();
            assert_eq!(file, format!("{tag}-history.json"));
            let parent = path
                .parent()
                .and_then(|dir| dir.file_name())
                .expect("历史路径应有数据目录")
                .to_string_lossy();
            assert_eq!(parent, "datas");
            assert_eq!(path.parent().unwrap(), history::data_dir().unwrap());
        }
    }

    #[test]
    fn profile_for_maps_only_ai_pages() {
        assert_eq!(
            profile_for(Page::Claude).map(|p| p.page),
            Some(Page::Claude)
        );
        assert_eq!(profile_for(Page::Gpt).map(|p| p.page), Some(Page::Gpt));
        for other in [Page::IpQuery, Page::IpScore, Page::DnsLeak] {
            assert!(profile_for(other).is_none());
        }
    }

    /// 集成冒烟（需真实网络，默认跳过）：`cargo test --lib probe_ai -- --ignored`
    /// 验证 Claude 页全链路编排（trace → iprisk/geoip → 受限判定 → 历史落盘）。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_claude_page_smoke() {
        let shared = crate::state::SharedState::new();
        crate::probe::spawn_home(shared.clone());
        {
            let mut state = shared.lock();
            state.app.page = Page::Claude;
        }
        super::spawn_for_page_if_needed(shared.clone());
        // 等编排完成（各步都有 8s 上限）
        for _ in 0..600 {
            {
                let state = shared.lock();
                if state
                    .ai
                    .page(Page::Claude)
                    .is_some_and(|page_state| matches!(page_state.phase, super::AiPhase::Done(_)))
                {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        let state = shared.lock();
        let Some(page_state) = state.ai.page(Page::Claude) else {
            panic!("Claude 页状态应存在");
        };
        let super::AiPhase::Done(outcome) = &page_state.phase else {
            panic!("编排应在时限内完成");
        };
        assert!(
            outcome
                .exit
                .as_ref()
                .and_then(|t| t.ip.as_deref())
                .is_some(),
            "claude.ai trace 应给出出口 IP"
        );
    }
}
