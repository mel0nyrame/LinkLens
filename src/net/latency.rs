//! 连通计时工具：TCP 连接 + TLS 握手完成计时（不等 body 下载）。
//!
//! 质量红线：
//! - 公网探测超时一律 8 秒；
//! - 淘宝/微信等目标以「连接 + TLS 握手完成」为准计时，绝不等待 body。
//!
//! 首页 6 目标小卡与 47 目标连通页共用本模块：预热 1 次 + 多轮测量取中位数。
//! 中位数与分档是纯函数（测试接缝），真实连接是薄 IO 壳。

use std::sync::Arc;
use std::time::Instant;

use url::Url;

use crate::net::http::PUBLIC_TIMEOUT;

/// 一轮测量的结果延迟（毫秒）或失败（`None`）。
pub type RoundResult = Option<u64>;

/// 多轮样本的中位数：升序取 `sorted[len/2]`（接口报告约定上中位）；空样本为 `None`。
pub fn median(samples: &[u64]) -> RoundResult {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    Some(sorted[sorted.len() / 2])
}

/// 延迟分档：快（<100ms）/ 良好（<400ms）/ 较慢（其余）/ 超时（失败）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LatencyTier {
    /// <100ms，绿。
    Fast,
    /// <400ms，浅绿。
    Good,
    /// 其余可达情况，黄。
    Slow,
    /// 探测失败 / 超时。
    Timeout,
}

/// 中位数分档；`None` 一律超时档。
pub fn tier(median: RoundResult) -> LatencyTier {
    match median {
        None => LatencyTier::Timeout,
        Some(ms) if ms < 100 => LatencyTier::Fast,
        Some(ms) if ms < 400 => LatencyTier::Good,
        Some(_) => LatencyTier::Slow,
    }
}

/// 失败档的展示文案。
pub const TIMEOUT_LABEL: &str = "超时";

/// 从 https URL 提取 `(host, port)`；缺省端口 443。仅接受 https。
pub fn parse_host_port(url_text: &str) -> Option<(String, u16)> {
    let url = Url::parse(url_text).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_string();
    if host.is_empty() {
        return None;
    }
    Some((host, url.port_or_known_default().unwrap_or(443)))
}

/// 多轮测量的节奏参数。
#[derive(Clone, Copy, Debug)]
pub struct RoundPlan {
    /// 隐藏预热次数（建立 TCP+TLS 连接，不计入样本）。
    pub warmup: u32,
    /// 计入样本的测量轮数。
    pub rounds: u32,
    /// 相邻两轮的最短间隔。
    pub min_interval: std::time::Duration,
    /// 轮间随机抖动上限（削峰，实际抖动确定性派生自轮次）。
    pub max_jitter: std::time::Duration,
}

impl RoundPlan {
    /// 首页 6 目标小卡节奏：预热 1 次 + 12 轮，轮间隔 ≥80ms（接口报告约定）。
    pub const HOME: RoundPlan = RoundPlan {
        warmup: 1,
        rounds: 12,
        min_interval: std::time::Duration::from_millis(80),
        max_jitter: std::time::Duration::from_millis(0),
    };

    /// 47 目标连通页节奏：预热 1 次 + 12 轮，轮间隔 ≥90ms + 0-140ms 抖动。
    pub const LINK: RoundPlan = RoundPlan {
        warmup: 1,
        rounds: 12,
        min_interval: std::time::Duration::from_millis(90),
        max_jitter: std::time::Duration::from_millis(140),
    };
}

/// 第 `round` 轮的确定性抖动（毫秒）：0..=max_jitter_ms，随轮次变化但不引入随机源。
fn jitter_ms(round: u32, max_jitter: std::time::Duration) -> u64 {
    let max = max_jitter.as_millis() as u64;
    if max == 0 {
        return 0;
    }
    (u64::from(round) * 97 + 13) % (max + 1)
}

/// 按 `plan` 对单目标执行「预热 + 多轮测量」，逐轮回调进度并返回全部测量轮结果。
///
/// `measure` 由调用方注入（真实 TLS 探针或测试桩）；每轮若早于最短间隔完成则补足间隔。
pub async fn probe_rounds<F, Fut>(
    measure: F,
    plan: &RoundPlan,
    mut on_round: impl FnMut(RoundResult),
) -> Vec<RoundResult>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = RoundResult>,
{
    for _ in 0..plan.warmup {
        measure().await;
    }
    let mut results = Vec::with_capacity(plan.rounds as usize);
    for round in 0..plan.rounds {
        let started = Instant::now();
        let result = measure().await;
        on_round(result);
        results.push(result);
        if round + 1 < plan.rounds {
            let gap = plan.min_interval.as_millis() as u64 + jitter_ms(round, plan.max_jitter);
            let elapsed = started.elapsed().as_millis() as u64;
            tokio::time::sleep(std::time::Duration::from_millis(gap.saturating_sub(elapsed))).await;
        }
    }
    results
}

/// TLS 握手计时探针：持有进程级共享的 rustls 客户端配置。
pub struct LatencyProbe {
    connector: Arc<tokio_rustls::TlsConnector>,
}

impl LatencyProbe {
    /// 构建探针（根证书取 webpki 根集）。
    pub fn new() -> LatencyProbe {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        LatencyProbe { connector: Arc::new(tokio_rustls::TlsConnector::from(Arc::new(config))) }
    }

    /// 对目标 URL 做一次「连接 + TLS 握手」计时；DNS 解析时间不计入。
    /// 总耗时上限为 8 秒红线；任何失败（解析、连接、握手、超时）为 `None`。
    pub async fn measure_url(&self, url: &str) -> RoundResult {
        let (host, port) = parse_host_port(url)?;
        self.measure(&host, port).await
    }

    /// 对 `host:port` 做一次「连接 + TLS 握手」计时；多个解析地址依次尝试。
    pub async fn measure(&self, host: &str, port: u16) -> RoundResult {
        let addrs = match tokio::net::lookup_host((host, port)).await {
            Ok(addrs) => addrs.collect::<Vec<_>>(),
            Err(_) => return None,
        };
        let Ok(server_name) = rustls::pki_types::ServerName::try_from(host.to_string()) else {
            return None;
        };
        for addr in addrs {
            let started = Instant::now();
            let attempt = async {
                let tcp = tokio::net::TcpStream::connect(addr).await.ok()?;
                tcp.set_nodelay(true).ok();
                // 握手完成即计时结束；连接随后直接丢弃，不等任何应用层响应
                self.connector.connect(server_name.clone(), tcp).await.ok()?;
                Some(())
            };
            if tokio::time::timeout(PUBLIC_TIMEOUT, attempt)
                .await
                .ok()
                .flatten()
                .is_some()
            {
                return Some(started.elapsed().as_millis() as u64);
            }
        }
        None
    }
}

impl Default for LatencyProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{LatencyTier, RoundPlan, TIMEOUT_LABEL, jitter_ms, median, parse_host_port, probe_rounds, tier};

    #[tokio::test]
    async fn warmup_is_excluded_and_rounds_are_reported_in_order() {
        let plan = RoundPlan {
            warmup: 2,
            rounds: 3,
            min_interval: std::time::Duration::from_millis(1),
            max_jitter: std::time::Duration::from_millis(0),
        };
        let counter = Arc::new(AtomicU32::new(0));
        let counter_cloned = Arc::clone(&counter);
        let reported = probe_rounds(
            || {
                let counter = Arc::clone(&counter_cloned);
                async move { Some(u64::from(counter.fetch_add(1, Ordering::SeqCst))) }
            },
            &plan,
            |_| {},
        )
        .await;
        // 预热 2 次不计入样本；测量轮按调用顺序得到 2、3、4
        assert_eq!(reported, vec![Some(2), Some(3), Some(4)]);
    }

    #[tokio::test]
    async fn failures_flow_through_as_none() {
        let plan = RoundPlan {
            warmup: 1,
            rounds: 3,
            min_interval: std::time::Duration::from_millis(1),
            max_jitter: std::time::Duration::from_millis(0),
        };
        let calls = Arc::new(AtomicU32::new(0));
        let calls_cloned = Arc::clone(&calls);
        let reported = Arc::new(AtomicU32::new(0));
        let reported_cloned = Arc::clone(&reported);
        let results = probe_rounds(
            || {
                let calls = Arc::clone(&calls_cloned);
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    if n == 0 { None } else { Some(50) } // 预热失败不影响测量轮
                }
            },
            &plan,
            |_| {
                reported_cloned.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert_eq!(results, vec![Some(50), Some(50), Some(50)]);
        assert_eq!(reported.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rounds_honor_min_interval() {
        let plan = RoundPlan {
            warmup: 0,
            rounds: 4,
            min_interval: std::time::Duration::from_millis(20),
            max_jitter: std::time::Duration::from_millis(0),
        };
        let started = std::time::Instant::now();
        probe_rounds(|| async { Some(1) }, &plan, |_| {}).await;
        // 4 轮之间至少 3 个 20ms 间隔（只断言下界，避免慢机抖动误报）
        assert!(started.elapsed() >= std::time::Duration::from_millis(60));
    }

    #[test]
    fn jitter_stays_in_range_and_varies() {
        for round in 0..24u32 {
            assert!(jitter_ms(round, std::time::Duration::from_millis(140)) <= 140);
        }
        assert_eq!(jitter_ms(0, std::time::Duration::from_millis(0)), 0);
        // 确定性：同轮次同结果
        assert_eq!(
            jitter_ms(7, std::time::Duration::from_millis(140)),
            jitter_ms(7, std::time::Duration::from_millis(140))
        );
    }

    #[test]
    fn home_and_link_plans_match_ticket_specs() {
        assert_eq!(RoundPlan::HOME.warmup, 1);
        assert_eq!(RoundPlan::HOME.rounds, 12);
        assert_eq!(RoundPlan::HOME.min_interval, std::time::Duration::from_millis(80));
        assert_eq!(RoundPlan::LINK.min_interval, std::time::Duration::from_millis(90));
        assert_eq!(RoundPlan::LINK.max_jitter, std::time::Duration::from_millis(140));
    }

    #[test]
    fn median_of_empty_is_none() {
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn median_of_single_sample_is_itself() {
        assert_eq!(median(&[42]), Some(42));
    }

    #[test]
    fn median_of_odd_count_takes_middle() {
        assert_eq!(median(&[300, 100, 200]), Some(200));
    }

    #[test]
    fn median_of_even_count_takes_upper_middle() {
        // 上游接口报告语义：sorted[floor(len/2)]，12 个样本取第 7 个
        assert_eq!(median(&[10, 20, 30, 40]), Some(30));
        let rounds: Vec<u64> = (1..=12).collect();
        assert_eq!(median(&rounds), Some(7));
    }

    #[test]
    fn median_ignores_input_order() {
        assert_eq!(median(&[500, 10, 999, 30, 60, 70]), Some(70));
    }

    #[test]
    fn tier_buckets_match_site_thresholds() {
        assert_eq!(tier(Some(0)), LatencyTier::Fast);
        assert_eq!(tier(Some(99)), LatencyTier::Fast);
        assert_eq!(tier(Some(100)), LatencyTier::Good);
        assert_eq!(tier(Some(399)), LatencyTier::Good);
        assert_eq!(tier(Some(400)), LatencyTier::Slow);
        assert_eq!(tier(Some(8000)), LatencyTier::Slow);
        assert_eq!(tier(None), LatencyTier::Timeout);
    }

    #[test]
    fn timeout_label_is_used_for_failures() {
        assert_eq!(TIMEOUT_LABEL, "超时");
    }

    #[test]
    fn parses_https_urls() {
        assert_eq!(parse_host_port("https://1.1.1.1/cdn-cgi/trace"), Some(("1.1.1.1".into(), 443)));
        assert_eq!(
            parse_host_port("https://res.wx.qq.com/a/wx_fed/assets/res/NTI4MWU5.ico"),
            Some(("res.wx.qq.com".into(), 443))
        );
        assert_eq!(parse_host_port("https://example.com:8443/x"), Some(("example.com".into(), 8443)));
    }

    #[test]
    fn rejects_non_https_and_garbage() {
        assert_eq!(parse_host_port("http://example.com/"), None);
        assert_eq!(parse_host_port(""), None);
        assert_eq!(parse_host_port("不是 URL"), None);
    }

    /// 集成冒烟（需真实网络，默认跳过）：`cargo test --lib -- --ignored`
    /// 验证 TLS 握手探针能对公网目标给出毫秒级结果。
    /// 边缘节点可能对无应用数据的握手限速，给两次机会。
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_tls_handshake_probe_returns_milliseconds() {
        let probe = super::LatencyProbe::new();
        let ms = match probe.measure_url("https://1.1.1.1/cdn-cgi/trace").await {
            Some(ms) => Some(ms),
            None => probe.measure_url("https://1.1.1.1/cdn-cgi/trace").await,
        };
        assert!(ms.is_some_and(|v| v > 0 && v <= 8_000), "实测延迟异常：{ms:?}");
    }
}
