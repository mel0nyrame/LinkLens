//! 泄漏检测编排：后台 tokio 任务把探测结果写进共享状态。
//!
//! - `spawn_dns_leak`：随机 token 子域走系统 resolver 发 UDP 53 查询（快速 5 轮 /
//!   深度 8 轮，间隔 600ms），静置 2s 后轮询回读解析器列表（最多 3 次），
//!   逐项 geoip 补旗后给出三态判定（报告 §3.6 全流程）；
//! - `spawn_webrtc_probe`：解析 3 个 STUN 服务器的 A+AAAA（IPv4/IPv6 都要采集），
//!   并发手写 STUN Binding，与 HTTP 出口对照判定（报告 §3.7）。
//!
//! 并发与节奏约束（红线）：公网收发超时一律 8 秒（`net::http` 客户端级
//! 与 `net::stun` 套接字超时）；页面按键 f/d 触发 DNS 探测，进入 WebRTC 页自动探测一次。

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use crossterm::event::KeyCode;
use futures_util::StreamExt;
use futures_util::stream;

use crate::app::Page;
use crate::detect::leak::{collect_candidates, judge_dns, judge_webrtc};
use crate::net::dnsleak;
use crate::net::geoip::{self, GeoIp};
use crate::net::http;
use crate::net::stun;
use crate::net::trace;
use crate::state::SharedState;
use crate::state_leak::{DnsLeakState, DnsMode, DnsPhase, ResolverEntry, WebrtcPhase, WebrtcState};

/// 出口参照的 trace 探测目标（报告 §3.6/§3.7 均用 1.1.1.1）。
const EGRESS_TRACE_HOST: &str = "1.1.1.1";

/// STUN 查询并发上限（服务器清单本身只有 3 个，双栈最多 6 个端点）。
const STUN_CONCURRENCY: usize = 6;

/// 页面级按键（与全局键位无冲突，事件循环在全局处理前调用）：
/// DNS 页 `f` 快速 / `d` 深度，WebRTC 页 `r` 重新探测。
pub fn handle_page_key(shared: SharedState, page: Page, code: KeyCode) {
    match (page, code) {
        (Page::DnsLeak, KeyCode::Char('f')) => spawn_dns_leak(shared, DnsMode::Fast),
        (Page::DnsLeak, KeyCode::Char('d')) => spawn_dns_leak(shared, DnsMode::Deep),
        (Page::WebRtc, KeyCode::Char('r')) => spawn_webrtc_probe(shared),
        _ => {}
    }
}

/// 发起一次 DNS 泄漏探测；进行中则忽略重复触发。
pub fn spawn_dns_leak(shared: SharedState, mode: DnsMode) {
    let already = {
        let mut state = shared.lock();
        if state.dns_leak.phase == DnsPhase::Running {
            true
        } else {
            state.dns_leak = DnsLeakState {
                phase: DnsPhase::Running,
                mode: Some(mode),
                rounds_total: mode.rounds(),
                ..DnsLeakState::default()
            };
            false
        }
    };
    if already {
        return;
    }
    tokio::spawn(run_dns_leak(shared, mode));
}

/// DNS 泄漏探测全流程（报告 §3.6）。
async fn run_dns_leak(shared: SharedState, mode: DnsMode) {
    let client = http::client();

    // 1. 出口参照：trace 拿出口 IP，geoip 补旗与国别
    let egress = fetch_egress(&client).await;
    {
        let mut state = shared.lock();
        state.dns_leak.egress_ip = egress.ip.clone();
        state.dns_leak.egress_flag = egress.flag;
        state.dns_leak.egress_geo = egress.geo;
        state.dns_leak.egress_country = egress.country.clone();
    }
    shared.notify();

    // 2. 真随机 token + 按「系统 resolver 配置」发询（红线：不写死公共 DNS）
    let token = dnsleak::generate_token();
    {
        let mut state = shared.lock();
        state.dns_leak.mode = Some(mode);
        state.dns_leak.rounds_total = mode.rounds();
    }
    let Ok(resolver) = dnsleak::system_resolver() else {
        fail_dns(&shared, "无法读取系统 resolver 配置，探测终止").await;
        return;
    };

    // 3. 逐轮触发权威解析：`<token>-<n>.d.ip.net.coffee`，轮间隔 600ms
    for round in 1..=mode.rounds() {
        dnsleak::flush_round(&resolver, &token, round).await;
        {
            let mut state = shared.lock();
            state.dns_leak.rounds_done = round;
        }
        shared.notify();
        if round < mode.rounds() {
            tokio::time::sleep(dnsleak::ROUND_INTERVAL).await;
        }
    }

    // 4. 静置 2s 后轮询回读（最多 3 次；空列表也是合法响应 → 已加密未暴露）
    tokio::time::sleep(dnsleak::RESULT_SETTLE).await;
    let mut servers: Vec<String> = Vec::new();
    let mut got_response = false;
    for _ in 0..dnsleak::RESULT_POLL_ATTEMPTS {
        if let Some(list) = dnsleak::fetch_dns_result(&client, &token).await {
            got_response = true;
            servers = list;
            if !servers.is_empty() {
                break;
            }
        }
        tokio::time::sleep(dnsleak::RESULT_POLL_INTERVAL).await;
    }
    if !got_response {
        fail_dns(&shared, "回读解析器列表失败，请检查网络后重试").await;
        return;
    }

    // 5. 解析器列表逐项 geoip 补旗与归属地（批量优先，失败退单查）
    let locations = fetch_geo_map(&client, &servers).await;
    let resolvers: Vec<ResolverEntry> = servers
        .iter()
        .map(|ip| {
            let geo = locations.get(ip);
            ResolverEntry {
                ip: ip.clone(),
                flag: geo
                    .map(|g| geoip::flag_emoji(&g.country_code))
                    .unwrap_or_default(),
                location: geo
                    .map(crate::net::cc::chinese_location)
                    .unwrap_or_default(),
            }
        })
        .collect();

    // 归属信息缺失时无法核验中国大陆 DNS，不把未知信息报告为干净。
    if egress.country.is_empty()
        || servers.iter().any(|ip| {
            locations
                .get(ip)
                .is_none_or(|geo| geo.country_code.is_empty())
        })
    {
        shared.lock().dns_leak.resolvers = resolvers;
        fail_dns(&shared, "出口或解析器归属地获取失败，无法判定泄漏，请重试").await;
        return;
    }

    // 6. 三态判定（纯函数照上游接口报告规则）
    let countries: Vec<&str> = servers
        .iter()
        .map(|ip| {
            locations
                .get(ip)
                .map(|g| g.country_code.as_str())
                .unwrap_or("")
        })
        .collect();
    let egress_country = {
        let state = shared.lock();
        state.dns_leak.egress_country.clone()
    };
    let verdict = judge_dns(&countries, &egress_country);
    {
        let mut state = shared.lock();
        state.dns_leak.resolvers = resolvers;
        state.dns_leak.verdict = Some(verdict);
        state.dns_leak.phase = DnsPhase::Done;
    }
    shared.notify();
}

/// 发起一次 WebRTC（STUN）探测；进行中则忽略重复触发。
pub fn spawn_webrtc_probe(shared: SharedState) {
    let already = {
        let mut state = shared.lock();
        if state.webrtc.phase == WebrtcPhase::Running {
            true
        } else {
            state.webrtc = WebrtcState {
                phase: WebrtcPhase::Running,
                ..WebrtcState::default()
            };
            false
        }
    };
    if already {
        return;
    }
    tokio::spawn(run_webrtc_probe(shared));
}

/// WebRTC 探测全流程（报告 §3.7）。
async fn run_webrtc_probe(shared: SharedState) {
    let client = http::client();

    // 1. HTTP 出口参照
    let egress = fetch_egress(&client).await;
    let egress_addr: Option<IpAddr> = egress.ip.as_deref().and_then(|ip| ip.parse().ok());
    {
        let mut state = shared.lock();
        state.webrtc.egress_ip = egress.ip.clone();
        state.webrtc.egress_flag = egress.flag;
        state.webrtc.egress_geo = egress.geo;
    }
    shared.notify();

    // 2. 解析 STUN 服务器：A + AAAA 都取，保证 IPv6 候选真的被采集（红线）
    let Ok(resolver) = dnsleak::system_resolver() else {
        fail_webrtc(&shared, "无法读取系统 resolver 配置，探测终止").await;
        return;
    };
    let mut endpoints: Vec<SocketAddr> = Vec::new();
    for server in stun::STUN_SERVERS {
        let Some((host, port)) = server
            .rsplit_once(':')
            .and_then(|(h, p)| p.parse::<u16>().ok().map(|port| (h.to_string(), port)))
        else {
            continue;
        };
        let Ok(lookup) = resolver.lookup_ip(host).await else {
            continue;
        };
        for ip in lookup.iter() {
            let endpoint = SocketAddr::new(ip, port);
            if !endpoints.contains(&endpoint) {
                endpoints.push(endpoint);
            }
        }
    }
    {
        let mut state = shared.lock();
        state.webrtc.queries_total = endpoints.len();
    }
    shared.notify();
    if endpoints.is_empty() {
        fail_webrtc(&shared, "无法解析任何 STUN 服务器地址，探测终止").await;
        return;
    }

    // 3. 并发发 Binding Request，边采集边刷新候选
    let mut mapped: Vec<IpAddr> = Vec::new();
    let mut queries_done = 0;
    let mut results = stream::iter(endpoints)
        .map(|endpoint| tokio::spawn(async move { stun::query_stun(endpoint).await }))
        .buffer_unordered(STUN_CONCURRENCY);
    while let Some(result) = results.next().await {
        queries_done += 1;
        if let Some(mapped_addr) = result.unwrap_or(None) {
            let ip = mapped_addr.ip();
            if !mapped.contains(&ip) {
                mapped.push(ip);
            }
        }
        {
            let mut state = shared.lock();
            state.webrtc.queries_done = queries_done;
            state.webrtc.candidates = collect_candidates(&mapped);
        }
        shared.notify();
    }

    // 4. 判定：任一公网 UDP 地址 ≠ HTTP 出口 → 可能泄漏
    let candidates = collect_candidates(&mapped);
    if egress_addr.is_none() {
        fail_webrtc(&shared, "HTTP 出口获取失败，无法对照公网 UDP 地址，请重试").await;
        return;
    }
    let public_ips: Vec<IpAddr> = candidates.iter().map(|candidate| candidate.ip).collect();
    let verdict = judge_webrtc(&public_ips, egress_addr);
    {
        let mut state = shared.lock();
        state.webrtc.candidates = candidates;
        state.webrtc.verdict = Some(verdict);
        state.webrtc.phase = WebrtcPhase::Done;
    }
    shared.notify();
}

/// 出口参照的汇总（IP + 国旗 + 中文归属地 + 国别码）。
struct Egress {
    ip: Option<String>,
    flag: String,
    geo: String,
    country: String,
}

/// 请求 1.1.1.1 trace 拿出口 IP，再 geoip 补齐归属信息。
async fn fetch_egress(client: &reqwest::Client) -> Egress {
    let ip = trace::fetch_trace(client, EGRESS_TRACE_HOST)
        .await
        .and_then(|t| t.ip);
    let geo = match &ip {
        Some(ip) => geoip::fetch_geoip(client, ip).await,
        None => None,
    };
    Egress {
        flag: geo
            .as_ref()
            .map(|g| geoip::flag_emoji(&g.country_code))
            .unwrap_or_default(),
        geo: geo
            .as_ref()
            .map(crate::net::cc::chinese_location)
            .unwrap_or_default(),
        country: geo
            .as_ref()
            .map(|g| g.country_code.clone())
            .unwrap_or_default(),
        ip,
    }
}

/// 批量取一组 IP 的归属地（geoip-batch，失败退单查）。
async fn fetch_geo_map(client: &reqwest::Client, ips: &[String]) -> HashMap<String, GeoIp> {
    let mut map = geoip::fetch_geoip_batch(client, ips)
        .await
        .unwrap_or_default();
    for ip in ips {
        if !map.contains_key(ip)
            && let Some(geo) = geoip::fetch_geoip(client, ip).await
        {
            // 单查兜底：批量接口漏掉的 IP 逐个补齐
            map.insert(ip.clone(), geo);
        }
    }
    map
}

/// 记 DNS 探测失败并结束。
async fn fail_dns(shared: &SharedState, message: &str) {
    {
        let mut state = shared.lock();
        state.dns_leak.phase = DnsPhase::Done;
        state.dns_leak.error = Some(message.to_string());
    }
    shared.notify();
}

/// 记 WebRTC 探测失败并结束。
async fn fail_webrtc(shared: &SharedState, message: &str) {
    {
        let mut state = shared.lock();
        state.webrtc.phase = WebrtcPhase::Done;
        state.webrtc.error = Some(message.to_string());
    }
    shared.notify();
}

#[cfg(test)]
mod tests {
    use super::{SharedState, run_dns_leak, run_webrtc_probe};
    use crate::state_leak::{DnsMode, DnsPhase, WebrtcPhase};

    /// 集成冒烟（需真实网络，默认跳过）：`cargo test --lib probe_leak -- --ignored`
    /// 验证 DNS 泄漏全流程（系统 resolver 触发 + 回读 + geoip + 判定）。
    /// 候选多少与成败取决于网络环境（代理/无网时允许失败态），这里只断言状态机完整。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_dns_leak_smoke() {
        let shared = SharedState::new();
        run_dns_leak(shared.clone(), DnsMode::Fast).await;
        let state = shared.lock();
        assert_eq!(state.dns_leak.phase, DnsPhase::Done);
        if state.dns_leak.error.is_some() {
            return; // 无网/代理环境：失败态即合法结果
        }
        assert!(state.dns_leak.verdict.is_some(), "成功收尾应有三态结论");
        assert_eq!(state.dns_leak.rounds_done, state.dns_leak.rounds_total);
        assert!(state.dns_leak.egress_ip.is_some(), "成功收尾应有出口参照");
    }

    /// 集成冒烟（需真实公网，默认跳过）：验证 STUN 采集与判定状态机。
    /// 采到候选与否取决于网络（代理环境 UDP 不通时合法地为 0）。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_webrtc_probe_smoke() {
        let shared = SharedState::new();
        run_webrtc_probe(shared.clone()).await;
        let state = shared.lock();
        assert_eq!(state.webrtc.phase, WebrtcPhase::Done);
        if state.webrtc.error.is_some() {
            return; // 无网/代理环境：失败态即合法结果
        }
        assert!(state.webrtc.verdict.is_some(), "成功收尾应有三态结论");
        assert_eq!(state.webrtc.queries_done, state.webrtc.queries_total);
    }
}
