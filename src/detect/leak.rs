//! 泄漏判定纯逻辑：DNS 三态判定与 WebRTC 泄漏判定。
//!
//! 判定规则依据接口报告前端（权威依据：net.coffee 接口报告 §3.6 / §3.7）：
//! - DNS：解析器列表为空 → 已加密未暴露；解析器含中国大陆且出口非中国大陆 → 泄漏；否则干净；
//! - WebRTC：任一公网 UDP 地址 ≠ HTTP 出口 IP → 可能泄漏；无公网地址 → 未暴露。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// DNS 泄漏三态判定结论。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DnsVerdict {
    /// DNS 泄漏：解析器出口含中国大陆，而流量出口不在中国大陆。
    Leak,
    /// 未检测到 DNS 解析器出口 IP，DNS 可能已加密（DoH/DoT）。
    Encrypted,
    /// 干净：未检测到 DNS 泄漏。
    Clean,
}

/// DNS 泄漏三态判定（纯函数）。
///
/// - 解析器国别列表为空 → [`DnsVerdict::Encrypted`]；
/// - 列表含中国大陆（国别码 `cn`）且出口国别非中国大陆 → [`DnsVerdict::Leak`]；
/// - 其余 → [`DnsVerdict::Clean`]。国别码大小写不敏感。
pub fn judge_dns(resolver_countries: &[&str], egress_country: &str) -> DnsVerdict {
    if resolver_countries.is_empty() {
        return DnsVerdict::Encrypted;
    }
    let has_cn = resolver_countries
        .iter()
        .any(|cc| cc.eq_ignore_ascii_case("cn"));
    if has_cn && !egress_country.eq_ignore_ascii_case("cn") {
        return DnsVerdict::Leak;
    }
    DnsVerdict::Clean
}

/// IPv4 私网/保留段判定：照浏览器 WebRTC 过滤清单（报告 §3.7）。
fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    // 清单：0/8、127/8、10/8、172.16/12、192.168/16、198.18/15、100.64/10
    let [o1, o2, _, _] = ip.octets();
    o1 == 0                                  // 未指定段 0/8（浏览器过滤 `0.`）
        || o1 == 127                         // 环回 127/8
        || ip.is_private()                   // RFC1918：10/8、172.16/12、192.168/16
        || (o1 == 198 && (18..=19).contains(&o2)) // 基准测试段 198.18/15
        || (o1 == 100 && o2 & 0b1100_0000 == 0b0100_0000) // CGNAT 100.64/10
}

/// IPv6 私网/保留段判定：环回 `::1` 与链路本地 `fe80::/10`（报告 §3.7）。
fn is_private_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback() || ip.is_unicast_link_local()
}

/// 地址是否属于私网/保留段（WebRTC 候选过滤清单，纯函数）。
pub fn is_private_addr(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
}

#[cfg(test)]
mod tests {
    use super::{DnsVerdict, IpAddr, is_private_addr, judge_dns};
    use std::net::Ipv4Addr;

    #[test]
    fn empty_resolver_list_means_encrypted() {
        assert_eq!(judge_dns(&[], "cn"), DnsVerdict::Encrypted);
        assert_eq!(judge_dns(&[], "us"), DnsVerdict::Encrypted);
    }

    #[test]
    fn cn_resolver_with_non_cn_egress_is_leak() {
        // 上游接口报告判定：存在 countryCode == 'cn' 的解析器且出口非 cn → 泄漏
        assert_eq!(judge_dns(&["us", "cn", "hk"], "jp"), DnsVerdict::Leak);
        assert_eq!(judge_dns(&["cn"], "us"), DnsVerdict::Leak);
    }

    #[test]
    fn cn_resolver_with_cn_egress_is_clean() {
        assert_eq!(judge_dns(&["cn", "cn"], "cn"), DnsVerdict::Clean);
    }

    #[test]
    fn foreign_resolvers_are_clean_regardless_of_egress() {
        assert_eq!(judge_dns(&["us", "hk"], "cn"), DnsVerdict::Clean);
        assert_eq!(judge_dns(&["jp"], "jp"), DnsVerdict::Clean);
    }

    #[test]
    fn country_codes_are_case_insensitive() {
        assert_eq!(judge_dns(&["CN"], "jp"), DnsVerdict::Leak);
        // 出口国别码大写同样按非中国大陆参与判定
        assert_eq!(judge_dns(&["cn"], "JP"), DnsVerdict::Leak);
        assert_eq!(judge_dns(&["us"], "JP"), DnsVerdict::Clean);
    }

    #[test]
    fn private_ipv4_ranges_per_browser_list() {
        let private: &[(&str, bool)] = &[
            ("0.0.0.0", true),         // 未指定段 0/8
            ("0.1.2.3", true),         // 0/8 整段（浏览器过滤 `0.`）
            ("10.0.0.1", true),        // RFC1918 10/8
            ("127.0.0.1", true),       // 环回 127/8
            ("172.15.255.255", false), // 172.16/12 之外
            ("172.16.0.1", true),      // RFC1918 172.16/12
            ("172.31.255.255", true),  // RFC1918 172.16/12 上界
            ("172.32.0.0", false),     // 172.16/12 之外
            ("192.168.1.1", true),     // RFC1918 192.168/16
            ("100.64.0.1", true),      // CGNAT 100.64/10（Tailscale 等）
            ("100.127.255.255", true), // CGNAT 上界
            ("100.128.0.0", false),    // CGNAT 之外
            ("198.18.0.1", true),      // 基准测试段 198.18/15
            ("198.19.255.255", true),  // 基准测试段上界
            ("198.20.0.0", false),     // 基准测试段之外
            ("8.8.8.8", false),        // 公网
            ("1.2.3.4", false),        // 公网
        ];
        for (text, expected) in private {
            let ip: IpAddr = text.parse().expect("测试 IP 应合法");
            assert_eq!(is_private_addr(ip), *expected, "{text}");
        }
    }

    #[test]
    fn private_ipv6_loopback_and_link_local() {
        assert!(is_private_addr("::1".parse().expect("合法")));
        assert!(is_private_addr("fe80::1".parse().expect("合法")));
        assert!(is_private_addr("febf::ffff".parse().expect("合法")));
        // fe80::/10 之外不拦
        assert!(!is_private_addr("fec0::1".parse().expect("合法")));
        assert!(!is_private_addr(
            "2606:4700:4700::1111".parse().expect("合法")
        ));
        assert!(!is_private_addr("::2".parse().expect("合法")));
        // std 的 Ipv4Addr::is_private 覆盖 RFC1918，这里交叉确认常量清单未被误改
        assert!(is_private_addr(IpAddr::from(Ipv4Addr::new(10, 1, 2, 3))));
    }
}
