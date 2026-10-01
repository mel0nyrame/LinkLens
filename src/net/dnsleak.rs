//! DNS 泄漏探测：随机 token 子域走系统 resolver 发 UDP 53 A 查询，回读解析器出口列表。
//!
//! 权威依据：net.coffee 接口报告 §2.7（`/api/dns/result/{token}`）与 §3.6（流程与判定）。
//! token 与探测域名：`<token>-<n>.d.ip.net.coffee`（`*.d.ip.net.coffee` 泛解析，
//! 权威 NS 会把「来查的 resolver IP」挂到 token 上）；随后轮询回读接口拿解析器列表。
//!
//! 质量红线：token 必须是真随机（OS CSPRNG），不得固定值或弱随机；
//! DNS 查询必须按系统 resolver 配置发询，否则测到的不是本机真实 DNS 链路。

use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::config::LookupIpStrategy;
use serde::Deserialize;

/// token 字符表：小写字母 + 数字（接口报告约定 36 字符）。
const TOKEN_ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// token 长度：24 位（接口报告约定）。
pub const TOKEN_LEN: usize = 24;

/// 拒绝采样上限：36 × 7 = 252。小于它的字节可均匀映射到 36 个字符，避免取模偏差。
const SAMPLE_LIMIT: u8 = (256 / TOKEN_ALPHABET.len() as u16 * TOKEN_ALPHABET.len() as u16) as u8;

/// 从字节流按拒绝采样提取 token 字符（纯函数，`generate_token` 的可测核心）。
///
/// 每个小于 `SAMPLE_LIMIT` 的字节映射为字符表中 `byte % 36` 号字符，其余字节丢弃；
/// 提取满 24 个字符即停。字节不足时返回能提取的部分（长度 < 24）。
pub fn token_from_bytes(bytes: &[u8]) -> String {
    let mut token = String::with_capacity(TOKEN_LEN);
    for &byte in bytes {
        if token.len() == TOKEN_LEN {
            break;
        }
        if byte < SAMPLE_LIMIT {
            let index = usize::from(byte % TOKEN_ALPHABET.len() as u8);
            token.push(TOKEN_ALPHABET[index] as char);
        }
    }
    token
}

/// 生成 24 位真随机 token：OS CSPRNG（getrandom）+ 拒绝采样；不用时间、不用固定种子。
pub fn generate_token() -> String {
    // 一次 128 字节平均可产出约 126 个字符，足够 24 位；循环兜底极端情况
    let mut buf = [0u8; 128];
    loop {
        getrandom::fill(&mut buf).expect("OS 随机源不可用");
        let token = token_from_bytes(&buf);
        if token.len() == TOKEN_LEN {
            return token;
        }
    }
}

/// 快速测试轮数（接口报告约定 5 轮）。
pub const FAST_ROUNDS: u32 = 5;
/// 深度测试轮数（接口报告约定 8 轮）。
pub const DEEP_ROUNDS: u32 = 8;
/// 相邻两轮探测的最短间隔（接口报告约定 600ms）。
pub const ROUND_INTERVAL: Duration = Duration::from_millis(600);
/// 轮询回读前的静置等待（接口报告约定 2 秒）。
pub const RESULT_SETTLE: Duration = Duration::from_secs(2);
/// 回读接口轮询次数上限（接口报告约定 3 次）。
pub const RESULT_POLL_ATTEMPTS: usize = 3;
/// 相邻两次回读轮询的间隔。
pub const RESULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 探测域名：`<token>-<n>.d.ip.net.coffee`（`n` 从 1 起，泛解析）。
///
/// 尾部带点确保按 FQDN 查询、不拼系统搜索域；每次查询都会驱动系统 resolver
/// 去问权威 NS，这正是「挂上 resolver IP」的触发动作。
pub fn probe_domain(token: &str, round: u32) -> String {
    format!("{token}-{round}.d.ip.net.coffee.")
}

/// `/api/dns/result/{token}` 响应体（报告 §2.7：未知 token 也返回 200 + 空列表）。
#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
pub struct DnsResult {
    #[serde(default)]
    pub token: String,
    /// 触发过查询的 DNS 解析器出口 IP 列表。
    pub dns_servers: Vec<String>,
}

/// 解析回读 JSON（纯函数）：非合法对象返回 `None`。
pub fn parse_dns_result(json: &str) -> Option<DnsResult> {
    serde_json::from_str(json).ok()
}

/// 构建按「系统 resolver 配置」发询的 tokio resolver（红线：绝不写死公共 DNS）。
///
/// 配置来源为操作系统 resolver 设置（macOS 为系统配置动态库，
/// Unix 为 `/etc/resolv.conf`）；仅把查询超时钉在公网探测 8 秒红线上。
pub fn system_resolver() -> Result<TokioResolver, String> {
    let mut builder = TokioResolver::builder_tokio()
        .map_err(|err| format!("读取系统 resolver 配置失败：{err}"))?;
    builder.options_mut().timeout = crate::net::http::PUBLIC_TIMEOUT;
    builder.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    builder
        .build()
        .map_err(|err| format!("构建系统 resolver 失败：{err}"))
}

/// 发起一轮探测：对 `<token>-<round>` 子域发 UDP 53 A/AAAA 查询（走系统 resolver）。
///
/// 只关心「resolver 去问了权威 NS」这个触发动作，解析成败不影响结果；
/// 每轮独立吞掉错误，避免单轮失败中断整次探测。
pub async fn flush_round(resolver: &TokioResolver, token: &str, round: u32) {
    let _ = resolver.lookup_ip(probe_domain(token, round)).await;
}

/// 轮询回读接口拿解析器列表；网络失败或响应不合法为 `None`。
pub async fn fetch_dns_result(client: &reqwest::Client, token: &str) -> Option<Vec<String>> {
    let text = client
        .get(format!(
            "{}/api/dns/result/{token}",
            crate::net::geoip::API_BASE
        ))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    Some(parse_dns_result(&text)?.dns_servers)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        DEEP_ROUNDS, FAST_ROUNDS, RESULT_POLL_ATTEMPTS, RESULT_SETTLE, ROUND_INTERVAL, TOKEN_LEN,
        generate_token, parse_dns_result, probe_domain, token_from_bytes,
    };

    #[test]
    fn token_from_bytes_worked_examples() {
        // 手算：字节 b 被接受时映射为字符表第 b % 36 位；≥ 252 的字节被拒绝
        assert_eq!(token_from_bytes(&[]), "");
        assert_eq!(token_from_bytes(&[0]), "a"); // 0 % 36 = 0 → 'a'
        assert_eq!(token_from_bytes(&[25]), "z"); // 25 % 36 = 25 → 'z'
        assert_eq!(token_from_bytes(&[26]), "0"); // 26 % 36 = 26 → '0'
        assert_eq!(token_from_bytes(&[35]), "9"); // 35 % 36 = 35 → '9'
        assert_eq!(token_from_bytes(&[36]), "a"); // 36 % 36 = 0 → 'a'
        assert_eq!(token_from_bytes(&[251]), "9"); // 251 % 36 = 35 → '9'
        assert_eq!(token_from_bytes(&[252]), ""); // 252 ≥ 252 → 拒绝
        assert_eq!(token_from_bytes(&[255]), ""); // 拒绝
    }

    #[test]
    fn token_from_bytes_stops_at_24_chars() {
        // 30 个可接受字节只产出 24 个字符
        let token = token_from_bytes(&[1; 30]);
        assert_eq!(token.len(), TOKEN_LEN);
        assert!(token.chars().all(|c| c == 'b'));
    }

    #[test]
    fn token_from_bytes_rejection_shrinks_output() {
        // 4 个字节里 2 个被拒绝 → 只产出 2 个字符（10→'k'，11→'l'）
        assert_eq!(token_from_bytes(&[10, 252, 11, 253]), "kl");
    }

    #[test]
    fn generate_token_has_official_shape() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_LEN, "token 应为 24 位：{token}");
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "token 只能含 [a-z0-9]：{token}"
        );
    }

    #[test]
    fn generate_token_is_not_fixed_or_weakly_random() {
        // 连续取 32 个 token：真随机下几乎不可能大量重复
        let tokens: Vec<String> = (0..32).map(|_| generate_token()).collect();
        let distinct: std::collections::HashSet<&String> = tokens.iter().collect();
        assert!(distinct.len() > 16, "token 疑似弱随机：{tokens:?}");
    }

    #[test]
    fn probe_domain_matches_official_pattern() {
        assert_eq!(
            probe_domain("sampledns0123456789ab", 1),
            "sampledns0123456789ab-1.d.ip.net.coffee."
        );
        assert_eq!(probe_domain("abc", 5), "abc-5.d.ip.net.coffee.");
        assert_eq!(probe_domain("abc", 8), "abc-8.d.ip.net.coffee.");
    }

    #[test]
    fn dns_round_constants_match_report() {
        // 报告 §3.6：快速 5 轮 / 深度 8 轮 / 轮间隔 600ms / 等 2s / 最多 3 次轮询
        assert_eq!(FAST_ROUNDS, 5);
        assert_eq!(DEEP_ROUNDS, 8);
        assert_eq!(ROUND_INTERVAL, Duration::from_millis(600));
        assert_eq!(RESULT_SETTLE, Duration::from_secs(2));
        assert_eq!(RESULT_POLL_ATTEMPTS, 3);
    }

    /// 报告 §2.7 的 `/api/dns/result` 脱敏响应样例（15 个解析器出口）。
    const RESULT_JSON: &str = r#"{"token": "sampledns0123456789ab", "dns_servers": ["192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4", "192.0.2.5", "192.0.2.6", "192.0.2.7", "192.0.2.8", "192.0.2.9", "192.0.2.10", "192.0.2.11", "192.0.2.12", "192.0.2.13", "192.0.2.14", "192.0.2.15"]}"#;

    #[test]
    fn parse_dns_result_from_report_sample() {
        let result = parse_dns_result(RESULT_JSON).expect("样例应可解析");
        assert_eq!(result.token, "sampledns0123456789ab");
        assert_eq!(result.dns_servers.len(), 15);
        assert_eq!(result.dns_servers[0], "192.0.2.1");
        assert_eq!(result.dns_servers[14], "192.0.2.15");
    }

    #[test]
    fn parse_dns_result_tolerates_empty_and_missing_fields() {
        // 未知 token 返回 200 + 空 dns_servers（报告 §2.7）
        let empty =
            parse_dns_result(r#"{"token": "x", "dns_servers": []}"#).expect("空列表应可解析");
        assert!(empty.dns_servers.is_empty());
        let missing = parse_dns_result(r#"{"dns_servers":[]}"#).expect("允许省略 token");
        assert!(missing.dns_servers.is_empty());
        assert_eq!(missing.token, "");
        assert_eq!(parse_dns_result("{}"), None, "缺解析器字段不是有效回读");
    }

    #[test]
    fn parse_dns_result_rejects_invalid_json() {
        assert_eq!(parse_dns_result(""), None);
        assert_eq!(parse_dns_result("not json"), None);
        assert_eq!(parse_dns_result("[1,2,3]"), None);
    }
}
