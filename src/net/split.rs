//! 网站分流出口探测（约 37 站）与出口去重汇总。
//!
//! 权威依据：net.coffee 接口报告 §3.1 与活站 `home-page.js` 的 `tests` 表（逐项转录）。
//! 三类手法：
//! 1. `cftrace`：GET `https://{domain}/cdn-cgi/trace` 取 `ip=`；
//! 2. 网易：HEAD 响应头 `cdn-user-ip`；
//! 3. 字节：HEAD 响应头 `x-request-ip`（可能是内网 NAT 地址，需公网优选）
//!    或 `x-response-cinfo`。
//!
//! 编排约束：并发 12、失败重试 2 次（间隔 2s/4s），geoip-batch 补旗，
//! 按出口 IP 去重汇总。活站清单现为 38 站（36 cftrace + 网易 + 字节），
//! 较票面「37 站」多一站（活站新增），以活站为准。

use std::time::Duration;

use super::trace;

/// 站点分类：国内 / 国际。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiteKind {
    Domestic,
    International,
}

/// 出口探测手法。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitMethod {
    /// GET `https://{domain}/cdn-cgi/trace`。
    Cftrace { domain: &'static str },
    /// 网易 CDN 头 `cdn-user-ip`。
    Netease,
    /// 字节 CDN 头 `x-request-ip` / `x-response-cinfo`。
    Bytedance { url: &'static str },
}

/// 一个分流测试站点。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SplitSite {
    pub name: &'static str,
    pub kind: SiteKind,
    pub method: SplitMethod,
}

/// 分流站点清单（活站 `home-page.js` 的 `tests` 表，2026-10-01 抓取）。
pub const SITES: &[SplitSite] = &[
    SplitSite {
        name: "网易",
        kind: SiteKind::Domestic,
        method: SplitMethod::Netease,
    },
    SplitSite {
        name: "字节跳动",
        kind: SiteKind::Domestic,
        method: SplitMethod::Bytedance {
            url: "https://perfops.byte-test.com/500b-bench.jpg",
        },
    },
    SplitSite {
        name: "Cloudflare中国",
        kind: SiteKind::Domestic,
        method: SplitMethod::Cftrace {
            domain: "www.cloudflare-cn.com",
        },
    },
    SplitSite {
        name: "高通中国",
        kind: SiteKind::Domestic,
        method: SplitMethod::Cftrace {
            domain: "www.qualcomm.cn",
        },
    },
    SplitSite {
        name: "discord.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "gateway.discord.gg",
        },
    },
    SplitSite {
        name: "x.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "x.com" },
    },
    SplitSite {
        name: "medium.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "medium.com",
        },
    },
    SplitSite {
        name: "signal.org",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "signal.org",
        },
    },
    SplitSite {
        name: "anthropic.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "anthropic.com",
        },
    },
    SplitSite {
        name: "claude.ai",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "claude.ai",
        },
    },
    SplitSite {
        name: "chatgpt.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "chatgpt.com",
        },
    },
    SplitSite {
        name: "openai.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "openai.com",
        },
    },
    SplitSite {
        name: "sora.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "sora.com" },
    },
    SplitSite {
        name: "grok.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "grok.com" },
    },
    SplitSite {
        name: "pixpix.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "pixpix.com",
        },
    },
    SplitSite {
        name: "perplexity.ai",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "www.perplexity.ai",
        },
    },
    SplitSite {
        name: "midjourney.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "midjourney.com",
        },
    },
    SplitSite {
        name: "mistral.ai",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "mistral.ai",
        },
    },
    SplitSite {
        name: "coinbase.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "coinbase.com",
        },
    },
    SplitSite {
        name: "www.okx.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "www.okx.com",
        },
    },
    SplitSite {
        name: "binance.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "www.binance.info",
        },
    },
    SplitSite {
        name: "crypto.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "crypto.com",
        },
    },
    SplitSite {
        name: "zoom.us",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "zoom.us" },
    },
    SplitSite {
        name: "1password.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "1password.com",
        },
    },
    SplitSite {
        name: "wise.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "wise.com" },
    },
    SplitSite {
        name: "poe.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace { domain: "poe.com" },
    },
    SplitSite {
        name: "notion.so",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "notion.so",
        },
    },
    SplitSite {
        name: "shopify.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "shopify.com",
        },
    },
    SplitSite {
        name: "godaddy.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "godaddy.com",
        },
    },
    SplitSite {
        name: "producthunt.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "producthunt.com",
        },
    },
    SplitSite {
        name: "cloudflare.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "www.cloudflare.com",
        },
    },
    SplitSite {
        name: "cloudflare cdnjs",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "cdnjs.cloudflare.com",
        },
    },
    SplitSite {
        name: "npm registry",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "registry.npmjs.org",
        },
    },
    SplitSite {
        name: "kali.download",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "kali.download",
        },
    },
    SplitSite {
        name: "unpkg.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "unpkg.com",
        },
    },
    SplitSite {
        name: "nodejs.org",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "nodejs.org",
        },
    },
    SplitSite {
        name: "gitlab.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "gitlab.com",
        },
    },
    SplitSite {
        name: "crunchyroll.com",
        kind: SiteKind::International,
        method: SplitMethod::Cftrace {
            domain: "crunchyroll.com",
        },
    },
];

/// 汇总里的一个去重出口：同一出口 IP 被哪些站点看到。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SplitExit {
    pub ip: String,
    pub country_code: String,
    pub sites: Vec<&'static str>,
}

/// 判断文本是否为合法 IP（IPv4 各段 0-255，或 IPv6 形式）。
pub fn is_valid_ip(text: &str) -> bool {
    if text.contains(':') {
        return !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
    }
    let octets: Vec<&str> = text.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|o| {
            !o.is_empty()
                && o.len() <= 3
                && o.chars().all(|c| c.is_ascii_digit())
                && o.parse::<u16>().is_ok_and(|v| v <= 255)
        })
}

/// 判断是否「适合展示的公网 IPv4」：合法且不在私网/环回/链路本地/CGNAT 段。
/// IPv6 一律按公网处理（分流的 IPv6 出口照常展示）。
pub fn is_public_ip(text: &str) -> bool {
    if !is_valid_ip(text) {
        return false;
    }
    if text.contains(':') {
        return true;
    }
    let octets: Vec<u16> = text.split('.').filter_map(|o| o.parse().ok()).collect();
    let [a, b, ..] = octets[..] else { return false };
    let (a, b) = (a as u8, b as u8);
    !matches!(
        (a, b),
        (0, _) | (10, _) | (127, _) | (169, 254) | (172, 16..=31) | (192, 168) | (100, 64..=127)
    ) && a < 224
}

/// 字节 CDN 双头优选：公网 `x-request-ip` 优先，其次公网 `x-response-cinfo`，
/// 两者都非公网时退回首个合法头；全部非法为 `None`。
pub fn pick_bytedance_ip(request_ip: Option<&str>, cinfo: Option<&str>) -> Option<String> {
    let request_ip = request_ip.filter(|ip| is_valid_ip(ip));
    let cinfo = cinfo.filter(|ip| is_valid_ip(ip));
    if let Some(ip) = request_ip.filter(|ip| is_public_ip(ip)) {
        return Some(ip.to_string());
    }
    if let Some(ip) = cinfo.filter(|ip| is_public_ip(ip)) {
        return Some(ip.to_string());
    }
    request_ip.or(cinfo).map(str::to_string)
}

/// 按出口 IP 去重汇总：保持首次出现顺序，站点名累加，国别码取首个非空值。
///
/// 记录为 `(站点名, 出口 IP, 小写国别码)`；国别码可为空串。
pub fn dedup_exits(records: &[(&'static str, String, String)]) -> Vec<SplitExit> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, SplitExit> = std::collections::HashMap::new();
    for (site, ip, cc) in records {
        let exit = map.entry(ip.clone()).or_insert_with(|| {
            order.push(ip.clone());
            SplitExit {
                ip: ip.clone(),
                country_code: String::new(),
                sites: Vec::new(),
            }
        });
        if exit.country_code.is_empty() {
            exit.country_code = cc.clone();
        }
        exit.sites.push(site);
    }
    order.into_iter().filter_map(|ip| map.remove(&ip)).collect()
}

/// 带重试地探测单站出口 IP：最多 1 次首发 + `delays` 逐项对应的重试（2s/4s）。
///
/// `attempt` 由调用方注入（真实 HTTP 探测或测试桩）。
pub async fn with_retries<F, Fut, T>(mut attempt: F, delays: &[Duration]) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    for attempt_index in 0..=delays.len() {
        if attempt_index > 0 {
            tokio::time::sleep(delays[attempt_index - 1]).await;
        }
        if let Some(value) = attempt().await {
            return Some(value);
        }
    }
    None
}

/// 网易 CDN 探测地址（`cdn-user-ip` 头）。
const NETEASE_URL: &str = "https://necaptcha.nosdn.127.net/ab7f4275c1744aa28e0a8f3a1c58c532.png";

/// 探测单站出口 IP（含重试）：cftrace 走 trace 手法，网易/字节走 CDN 响应头。
pub async fn probe_site(client: &reqwest::Client, site: &SplitSite) -> Option<String> {
    with_retries(
        || async { probe_site_once(client, site).await },
        &[Duration::from_secs(2), Duration::from_secs(4)],
    )
    .await
}

/// 单次探测（无重试）。
async fn probe_site_once(client: &reqwest::Client, site: &SplitSite) -> Option<String> {
    match site.method {
        SplitMethod::Cftrace { domain } => trace::fetch_trace(client, domain).await?.ip,
        SplitMethod::Netease => header_ip(client, NETEASE_URL, &["cdn-user-ip"]).await,
        SplitMethod::Bytedance { url } => {
            let headers = head_headers(client, url).await?;
            pick_bytedance_ip(headers.get("x-request-ip"), headers.get("x-response-cinfo"))
        }
    }
}

/// HEAD 请求并读取指定响应头中的合法 IP（取第一个命中的头）。
async fn header_ip(client: &reqwest::Client, url: &str, names: &[&str]) -> Option<String> {
    let headers = head_headers(client, url).await?;
    names
        .iter()
        .filter_map(|name| headers.get(name))
        .map(str::to_string)
        .find(|ip| is_valid_ip(ip))
}

/// HEAD 请求，返回目标响应头的字符串视图（缺失为 `None`）。
struct HeaderView {
    values: Vec<(String, String)>,
}

impl HeaderView {
    fn get(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim())
    }
}

async fn head_headers(client: &reqwest::Client, url: &str) -> Option<HeaderView> {
    let response = client.head(url).send().await.ok()?;
    let values = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    Some(HeaderView { values })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        SITES, SiteKind, SplitMethod, dedup_exits, is_public_ip, is_valid_ip, pick_bytedance_ip,
        with_retries,
    };
    use crate::net::split::SplitExit;

    #[test]
    fn site_list_matches_live_site_shape() {
        // 活站 2026-10-01：4 国内（网易/字节/CF 中国/高通）+ 34 国际 = 38 站
        assert_eq!(SITES.len(), 38);
        assert_eq!(
            SITES
                .iter()
                .filter(|s| s.kind == SiteKind::Domestic)
                .count(),
            4
        );
        let cftrace = SITES
            .iter()
            .filter(|s| matches!(s.method, SplitMethod::Cftrace { .. }))
            .count();
        assert_eq!(cftrace, 36);
        assert_eq!(
            SITES
                .iter()
                .filter(|s| s.method == SplitMethod::Netease)
                .count(),
            1
        );
        assert_eq!(
            SITES
                .iter()
                .filter(|s| matches!(s.method, SplitMethod::Bytedance { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn site_names_are_unique() {
        for (i, a) in SITES.iter().enumerate() {
            for b in &SITES[i + 1..] {
                assert_ne!(a.name, b.name, "站点名重复：{}", a.name);
            }
        }
    }

    #[test]
    fn cftrace_domains_are_unique() {
        let mut domains: Vec<&str> = SITES
            .iter()
            .filter_map(|s| match s.method {
                SplitMethod::Cftrace { domain } => Some(domain),
                _ => None,
            })
            .collect();
        domains.sort_unstable();
        let len = domains.len();
        domains.dedup();
        assert_eq!(domains.len(), len, "cftrace 域名应互不重复");
    }

    #[test]
    fn valid_and_invalid_ips() {
        assert!(is_valid_ip("192.0.2.216"));
        assert!(is_valid_ip("2606:4700:4700::1111"));
        assert!(!is_valid_ip("10.0.0.999"));
        assert!(!is_valid_ip(""));
        assert!(!is_valid_ip("not-an-ip"));
    }

    #[test]
    fn private_ranges_are_not_public() {
        for private in [
            "10.0.0.216",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "127.0.0.1",
            "0.0.0.0",
            "100.64.0.1",
            "169.254.1.1",
        ] {
            assert!(!is_public_ip(private), "{private} 不应算公网");
        }
        for public in ["192.0.2.216", "8.8.8.8", "172.32.0.1", "100.128.0.1"] {
            assert!(is_public_ip(public), "{public} 应算公网");
        }
    }

    #[test]
    fn bytedance_header_prefers_public_request_ip() {
        assert_eq!(
            pick_bytedance_ip(Some("203.0.113.7"), Some("198.51.100.9")),
            Some("203.0.113.7".to_string())
        );
    }

    #[test]
    fn bytedance_header_falls_back_to_cinfo_when_request_ip_is_private_nat() {
        // 报告 §6 实测：x-request-ip 是内网大 NAT 地址
        assert_eq!(
            pick_bytedance_ip(Some("10.0.0.216"), Some("198.51.100.176")),
            Some("198.51.100.176".to_string())
        );
    }

    #[test]
    fn bytedance_header_keeps_private_value_when_nothing_better() {
        assert_eq!(
            pick_bytedance_ip(Some("10.0.0.216"), Some("10.0.0.1")),
            Some("10.0.0.216".to_string())
        );
        assert_eq!(pick_bytedance_ip(Some("垃圾"), None), None);
        assert_eq!(pick_bytedance_ip(None, None), None);
    }

    #[test]
    fn dedup_groups_by_ip_in_first_seen_order() {
        let exits = dedup_exits(&[
            ("claude.ai", "203.0.113.101".to_string(), "us".to_string()),
            ("chatgpt.com", "203.0.113.101".to_string(), String::new()),
            (
                "www.binance.info",
                "198.51.100.3".to_string(),
                "jp".to_string(),
            ),
            ("x.com", "203.0.113.101".to_string(), "us".to_string()),
        ]);
        assert_eq!(
            exits,
            vec![
                SplitExit {
                    ip: "203.0.113.101".into(),
                    country_code: "us".into(),
                    sites: vec!["claude.ai", "chatgpt.com", "x.com"],
                },
                SplitExit {
                    ip: "198.51.100.3".into(),
                    country_code: "jp".into(),
                    sites: vec!["www.binance.info"],
                },
            ]
        );
    }

    #[test]
    fn dedup_fills_country_code_from_first_non_empty() {
        let exits = dedup_exits(&[
            ("a.com", "1.1.1.1".to_string(), String::new()),
            ("b.com", "1.1.1.1".to_string(), "sg".to_string()),
        ]);
        assert_eq!(exits[0].country_code, "sg");
    }

    #[tokio::test]
    async fn retries_succeed_after_failures_and_exhaust() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        // 第 2 次尝试成功
        let calls = Arc::new(AtomicU32::new(0));
        let calls_cloned = Arc::clone(&calls);
        let value = with_retries(
            || {
                let calls = Arc::clone(&calls_cloned);
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        None
                    } else {
                        Some("1.2.3.4".to_string())
                    }
                }
            },
            &[Duration::from_millis(1), Duration::from_millis(1)],
        )
        .await;
        assert_eq!(value, Some("1.2.3.4".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // 全部失败：首发 + 2 重试 = 3 次
        let calls = Arc::new(AtomicU32::new(0));
        let calls_cloned = Arc::clone(&calls);
        let value: Option<String> = with_retries(
            || {
                let calls = Arc::clone(&calls_cloned);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    None
                }
            },
            &[Duration::from_millis(1), Duration::from_millis(1)],
        )
        .await;
        assert_eq!(value, None);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
