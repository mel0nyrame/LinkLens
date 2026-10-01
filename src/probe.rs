//! 探测编排：后台 tokio 任务把 `net` 层探测结果写进共享状态。
//!
//! - `spawn_home`：应用启动即运行——国内双源出口 + Cloudflare 出口三张卡、
//!   首页 6 目标连通小卡、分流出口探测与去重汇总；
//! - `spawn_link`：首次进入连通页时运行一次——全部目标的分组连通测量。
//!
//! 并发与节奏约束：
//! - 公网探测超时一律 8 秒（`net::http` 客户端级）；
//! - 连通计时预热 1 次，首页 12 轮、连通页 8 轮取中位数（`net::latency::RoundPlan`）；
//! - 分流并发 12、失败重试 2 次（`net::split::probe_site` 内置 2s/4s）。

use futures_util::StreamExt;
use futures_util::stream;

use crate::net::cc::{GeoCandidate, validate_geo};
use crate::net::cn_source;
use crate::net::geoip::{self, GeoIp};
use crate::net::http;
use crate::net::iprisk;
use crate::net::latency::{LatencyProbe, RoundPlan, probe_rounds};
use crate::net::split;
use crate::net::targets::TARGETS;
use crate::net::trace;
use crate::state::{EgressCard, EgressPhase, LatencyState, SharedState, SplitSiteState};

/// 首页 6 目标连通小卡（名称、标注、URL；接口报告约定目标）。
pub struct HomeLatencyTarget {
    pub name: &'static str,
    pub tag: &'static str,
    pub url: &'static str,
}

/// 首页 6 目标：国内外各半，顺序即小卡顺序。
pub const HOME_LATENCY_TARGETS: [HomeLatencyTarget; 6] = [
    HomeLatencyTarget {
        name: "字节跳动",
        tag: "国内",
        url: "https://perfops.byte-test.com/500b-bench.jpg",
    },
    HomeLatencyTarget {
        name: "淘宝",
        tag: "国内",
        url: "https://www.taobao.com/favicon.ico",
    },
    HomeLatencyTarget {
        name: "微信",
        tag: "国内",
        url: "https://res.wx.qq.com/a/wx_fed/assets/res/NTI4MWU5.ico",
    },
    HomeLatencyTarget {
        name: "GitHub",
        tag: "国际",
        url: "https://github.com/generate_204",
    },
    HomeLatencyTarget {
        name: "Cloudflare",
        tag: "国际",
        url: "https://1.1.1.1/cdn-cgi/trace",
    },
    HomeLatencyTarget {
        name: "YouTube",
        tag: "国际",
        url: "https://www.youtube.com/generate_204",
    },
];

/// 分流探测并发上限（接口报告约定）。
const SPLIT_CONCURRENCY: usize = 12;

/// 启动首页探测任务（应用启动时调用一次）。
pub fn spawn_home(shared: SharedState) {
    tokio::spawn(async move {
        let client = http::client();
        let probe = LatencyProbe::new();

        let egress_task = probe_home_egress(&client, shared.clone());
        let latency_task = probe_home_latency(&probe, shared.clone());
        let split_task = probe_home_split(&client, shared.clone());
        let ((), (), ()) = tokio::join!(egress_task, latency_task, split_task);
    });
}

/// 启动连通目标清单的探测（首次进入连通页时调用一次；幂等）。
pub fn spawn_link(shared: SharedState) {
    let already = {
        let mut state = shared.lock();
        if state.link.started {
            true
        } else {
            state.link.started = true;
            state.link.targets = TARGETS.iter().map(|t| LatencyState::new(t.name)).collect();
            false
        }
    };
    if already {
        return;
    }
    tokio::spawn(async move {
        let probe = LatencyProbe::new();
        // 并发池 9（接口报告约定）：每个目标独立走「预热 + 8 轮」
        stream::iter(TARGETS.iter().enumerate())
            .for_each_concurrent(9, |(index, target)| {
                let probe = &probe;
                let shared = shared.clone();
                async move {
                    let on_round = |result| {
                        let mut state = shared.lock();
                        state.link.targets[index].push_round(result);
                        drop(state);
                        shared.notify();
                    };
                    probe_rounds(|| probe.measure_url(target.url), &RoundPlan::LINK, on_round)
                        .await;
                    let mut state = shared.lock();
                    state.link.targets[index].done = true;
                    drop(state);
                    shared.notify();
                }
            })
            .await;
    });
}

/// 首页出口三卡：国内双源（主/备）+ Cloudflare 出口。
async fn probe_home_egress(client: &reqwest::Client, shared: SharedState) {
    let cards = fetch_egress_cards(client).await;
    shared.lock().home.egress = EgressPhase::Ready(cards);
    shared.notify();
}

/// 并行采集国内双源和 Cloudflare 出口并补全卡片；不写页面状态。
pub(crate) async fn fetch_egress_cards(client: &reqwest::Client) -> Vec<EgressCard> {
    let cn_138 = cn_source::fetch_ip138(client);
    let cn_ipcn = cn_source::fetch_my_ip_cn(client);
    let cf_trace = trace::fetch_trace(client, "1.1.1.1");
    let (r138, ripcn, rcf) = tokio::join!(cn_138, cn_ipcn, cf_trace);

    let mut cards: Vec<EgressCard> = Vec::new();
    match (r138, ripcn) {
        (Some(a), Some(b)) if a.ip != b.ip => {
            // 双源 IP 不同：主出口 + 备用出口分卡
            if let Some(card) = build_cn_card(client, "主出口", a, "iP138.com").await {
                cards.push(card);
            }
            if let Some(card) = build_cn_card(client, "备用出口", b, "IP.cn").await {
                cards.push(card);
            }
        }
        (Some(a), Some(b)) => {
            // 双源同 IP：单卡，来源并列标注，归属地择优校验
            let candidates = vec![
                GeoCandidate {
                    geo: a.location.clone(),
                    source: "iP138.com",
                },
                GeoCandidate {
                    geo: b.location.clone(),
                    source: "IP.cn",
                },
            ];
            let geoip_data = geoip::fetch_geoip(client, &a.ip).await;
            let resolved = validate_geo(geoip_data.as_ref(), &candidates);
            let source = if resolved.source == crate::net::cc::CORRECTED_SOURCE {
                resolved.source.clone()
            } else {
                "iP138.com / IP.cn".to_string()
            };
            let mut card =
                assemble_card("主出口", &a.ip, &resolved.geo, &source, geoip_data.as_ref());
            card.badges = fetch_badges(client, &a.ip).await;
            cards.push(card);
        }
        (Some(a), None) => {
            if let Some(card) = build_cn_card(client, "主出口", a, "iP138.com").await {
                cards.push(card);
            }
        }
        (None, Some(b)) => {
            if let Some(card) = build_cn_card(client, "主出口", b, "IP.cn").await {
                cards.push(card);
            }
        }
        (None, None) => {}
    }

    // Cloudflare 出口卡（无国内源文本，归属地直接由 geoip 拼中文）
    if let Some(ip) = rcf.and_then(|trace_data| trace_data.ip) {
        let geoip_data = geoip::fetch_geoip(client, &ip).await;
        let geo = geoip_data
            .as_ref()
            .map(crate::net::cc::chinese_location)
            .unwrap_or_default();
        let mut card = assemble_card(
            "Cloudflare 出口",
            &ip,
            &geo,
            "Cloudflare trace",
            geoip_data.as_ref(),
        );
        card.badges = fetch_badges(client, &ip).await;
        cards.push(card);
    }

    cards
}

/// 组装国内出口卡：geoip 补旗 + 归属地交叉校验 + iprisk 属性徽章。
async fn build_cn_card(
    client: &reqwest::Client,
    label: &'static str,
    source: cn_source::CnSource,
    source_label: &'static str,
) -> Option<EgressCard> {
    let geoip_data = geoip::fetch_geoip(client, &source.ip).await;
    let candidates = vec![GeoCandidate {
        geo: source.location.clone(),
        source: source_label,
    }];
    let resolved = validate_geo(geoip_data.as_ref(), &candidates);
    let mut card = assemble_card(
        label,
        &source.ip,
        &resolved.geo,
        resolved.source.as_str(),
        geoip_data.as_ref(),
    );
    card.badges = fetch_badges(client, &source.ip).await;
    Some(card)
}

/// iprisk 属性徽章（单个；无判定时不显示）。
async fn fetch_badges(client: &reqwest::Client, ip: &str) -> Vec<&'static str> {
    iprisk::fetch_iprisk(client, ip)
        .await
        .and_then(|risk| risk.property_badge().map(|badge| vec![badge]))
        .unwrap_or_default()
}

/// 组装出口卡骨架（不含徽章）。
fn assemble_card(
    label: &str,
    ip: &str,
    geo: &str,
    source_label: &str,
    geoip_data: Option<&GeoIp>,
) -> EgressCard {
    EgressCard {
        label: label.to_string(),
        ip: ip.to_string(),
        geo: geo.to_string(),
        source: source_label.to_string(),
        flag: geoip_data
            .map(|g| geoip::flag_emoji(&g.country_code))
            .unwrap_or_default(),
        badges: Vec::new(),
    }
}

/// 首页 6 目标连通小卡：全目标并行，各自「预热 1 次 + 12 轮取中位数」。
async fn probe_home_latency(probe: &LatencyProbe, shared: SharedState) {
    {
        let mut state = shared.lock();
        state.home.latency = HOME_LATENCY_TARGETS
            .iter()
            .map(|t| LatencyState::new(t.name))
            .collect();
    }
    shared.notify();

    stream::iter(HOME_LATENCY_TARGETS.iter().enumerate())
        .for_each_concurrent(HOME_LATENCY_TARGETS.len(), |(index, target)| {
            let probe = &probe;
            let shared = shared.clone();
            async move {
                let on_round = |result| {
                    let mut state = shared.lock();
                    state.home.latency[index].push_round(result);
                    drop(state);
                    shared.notify();
                };
                probe_rounds(|| probe.measure_url(target.url), &RoundPlan::HOME, on_round).await;
                let mut state = shared.lock();
                state.home.latency[index].done = true;
                drop(state);
                shared.notify();
            }
        })
        .await;
}

/// 分流目标清单：并发 12 探测 → geoip-batch 补旗 → 出口去重汇总。
async fn probe_home_split(client: &reqwest::Client, shared: SharedState) {
    {
        let mut state = shared.lock();
        state.home.split = split::SITES
            .iter()
            .map(|_| SplitSiteState::default())
            .collect();
    }
    shared.notify();

    stream::iter(split::SITES.iter().enumerate())
        .for_each_concurrent(SPLIT_CONCURRENCY, |(index, site)| {
            let client = &client;
            let shared = shared.clone();
            async move {
                let ip = split::probe_site(client, site).await;
                let mut state = shared.lock();
                state.home.split[index] = SplitSiteState { ip };
                drop(state);
                shared.notify();
            }
        })
        .await;

    build_split_summary(client, shared).await;
}

/// 汇总阶段：收集去重出口 → geoip-batch 补旗 → 写回共享状态。
async fn build_split_summary(client: &reqwest::Client, shared: SharedState) {
    let records: Vec<(&'static str, String, String)> = {
        let state = shared.lock();
        split::SITES
            .iter()
            .zip(state.home.split.iter())
            .filter_map(|(site, result)| {
                let ip = result.ip.clone()?;
                Some((site.name, ip, String::new()))
            })
            .collect()
    };
    let unique_ips: Vec<String> = {
        let mut seen = Vec::new();
        for (_, ip, _) in &records {
            if !seen.contains(ip) {
                seen.push(ip.clone());
            }
        }
        seen
    };

    // geoip-batch 批量补旗；失败时退回逐个单查
    let mut country_codes: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(map) = geoip::fetch_geoip_batch(client, &unique_ips).await {
        for (ip, geo) in map {
            country_codes.insert(ip, geo.country_code);
        }
    } else {
        for ip in &unique_ips {
            if let Some(geo) = geoip::fetch_geoip(client, ip).await {
                country_codes.insert(ip.clone(), geo.country_code);
            }
        }
    }

    let records: Vec<(&'static str, String, String)> = records
        .into_iter()
        .map(|(site, ip, _)| {
            let cc = country_codes.get(&ip).cloned().unwrap_or_default();
            (site, ip, cc)
        })
        .collect();

    let mut state = shared.lock();
    state.home.split_summary = split::dedup_exits(&records);
    state.home.split_done = true;
    drop(state);
    shared.notify();
}

#[cfg(test)]
mod tests {
    use super::{SharedState, http, probe_home_egress};
    use crate::net::split;
    use crate::state::EgressPhase;

    /// 集成冒烟（需真实网络，默认跳过）：`cargo test --lib probe -- --ignored`
    /// 验证首页出口三卡的完整编排（双源 → geoip → 交叉校验 → iprisk）。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_home_egress_smoke() {
        let shared = SharedState::new();
        let client = http::client();
        probe_home_egress(&client, shared.clone()).await;
        let state = shared.lock();
        let EgressPhase::Ready(cards) = &state.home.egress else {
            panic!("探测结束应进入 Ready");
        };
        assert!(!cards.is_empty(), "至少应有一张出口卡");
        for card in cards {
            assert!(!card.ip.is_empty(), "出口卡应有 IP：{:?}", card.label);
        }
    }

    /// 集成冒烟（需真实网络，默认跳过）：网易站的 cdn-user-ip 头探测链路。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_split_netease_smoke() {
        let client = http::client();
        let site = split::SITES.first().copied().expect("清单非空");
        assert_eq!(site.name, "网易");
        let ip = split::probe_site(&client, &site).await;
        assert!(
            ip.as_ref()
                .is_some_and(|v| crate::net::split::is_valid_ip(v)),
            "网易应给出合法出口 IP：{ip:?}"
        );
    }
}
