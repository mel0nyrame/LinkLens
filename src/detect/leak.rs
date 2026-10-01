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

/// WebRTC 候选类型（接口报告约定三分类）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CandidateKind {
    /// 经 STUN 反射得到的公网地址。
    PublicStun,
    /// 经 TURN 中继得到的地址（本工具无 TURN，仅保留分类完整性）。
    Relay,
    /// 本地/主机候选（含私网地址）。
    Local,
}

impl CandidateKind {
    /// 展示文案（接口报告约定）。
    pub fn label(self) -> &'static str {
        match self {
            CandidateKind::PublicStun => "公网 (STUN)",
            CandidateKind::Relay => "中继 (TURN)",
            CandidateKind::Local => "本地",
        }
    }
}

/// 一条 WebRTC 候选（已过私网过滤，供展示）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WebrtcCandidate {
    pub ip: IpAddr,
    pub kind: CandidateKind,
}

/// 候选分类（纯函数）：STUN 反射地址 → 公网 (STUN)；私网地址 → 本地；
/// 其余（主机候选的公网地址等）→ 本地。中继需 TURN，本工具不产生。
pub fn classify_candidate(ip: IpAddr, reflexive: bool) -> CandidateKind {
    if reflexive && !is_private_addr(ip) {
        return CandidateKind::PublicStun;
    }
    CandidateKind::Local
}

/// 依私网段清单过滤并整理 STUN 采集到的候选清单（纯函数）：私网剔除、按 IP 去重保序。
pub fn collect_candidates(stun_mapped: &[IpAddr]) -> Vec<WebrtcCandidate> {
    let mut out: Vec<WebrtcCandidate> = Vec::new();
    for &ip in stun_mapped {
        if is_private_addr(ip) || out.iter().any(|c| c.ip == ip) {
            continue;
        }
        out.push(WebrtcCandidate {
            ip,
            kind: classify_candidate(ip, true),
        });
    }
    out
}

/// WebRTC 泄漏三态判定结论。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WebrtcVerdict {
    /// 可能泄漏：公网 UDP 地址与 HTTP 出口不一致。
    MaybeLeak,
    /// 未发现公网 UDP 地址：WebRTC 已禁用或未暴露。
    NoPublic,
    /// 干净：公网 UDP 地址与 HTTP 出口一致。
    Clean,
}

/// WebRTC 泄漏判定（纯函数，票面规则 + 上游接口报告规则）。
///
/// - 无公网 UDP 地址 → [`WebrtcVerdict::NoPublic`]；
/// - HTTP 出口已知：任一公网 UDP 地址 ≠ 出口 → [`WebrtcVerdict::MaybeLeak`]，否则干净；
/// - HTTP 出口未知：出现 ≥2 个不同公网地址（上游接口报告规则「多个不同公网 STUN IP」）→
///   [`WebrtcVerdict::MaybeLeak`]，否则干净。
pub fn judge_webrtc(public_ips: &[IpAddr], egress: Option<IpAddr>) -> WebrtcVerdict {
    if public_ips.is_empty() {
        return WebrtcVerdict::NoPublic;
    }
    match egress {
        Some(exit) => {
            if public_ips.iter().any(|&ip| ip != exit) {
                WebrtcVerdict::MaybeLeak
            } else {
                WebrtcVerdict::Clean
            }
        }
        None => {
            let distinct = public_ips.iter().collect::<std::collections::HashSet<_>>();
            if distinct.len() >= 2 {
                WebrtcVerdict::MaybeLeak
            } else {
                WebrtcVerdict::Clean
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CandidateKind, DnsVerdict, IpAddr, WebrtcVerdict, classify_candidate, collect_candidates,
        is_private_addr, judge_dns, judge_webrtc,
    };
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

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("测试 IP 应合法")
    }

    #[test]
    fn reflexive_public_address_is_public_stun() {
        assert_eq!(
            classify_candidate(ip("8.8.8.8"), true),
            CandidateKind::PublicStun
        );
        assert_eq!(
            classify_candidate(ip("2606:4700:4700::1111"), true),
            CandidateKind::PublicStun
        );
    }

    #[test]
    fn private_or_non_reflexive_addresses_are_local() {
        assert_eq!(
            classify_candidate(ip("192.168.1.2"), true),
            CandidateKind::Local
        );
        assert_eq!(
            classify_candidate(ip("8.8.8.8"), false),
            CandidateKind::Local
        );
    }

    #[test]
    fn collect_candidates_filters_private_and_dedups_in_order() {
        let candidates = collect_candidates(&[
            ip("8.8.8.8"),
            ip("10.0.0.5"),        // 私网剔除
            ip("8.8.8.8"),         // 去重
            ip("2606:4700::1111"), // IPv6 保留
            ip("fe80::1"),         // 链路本地剔除
        ]);
        assert_eq!(
            candidates
                .iter()
                .map(|c| c.ip.to_string())
                .collect::<Vec<_>>(),
            vec!["8.8.8.8", "2606:4700::1111"]
        );
        assert!(
            candidates
                .iter()
                .all(|c| c.kind == CandidateKind::PublicStun)
        );
    }

    #[test]
    fn collect_candidates_empty_input_is_empty() {
        assert!(collect_candidates(&[]).is_empty());
    }

    #[test]
    fn webrtc_no_public_address_means_not_exposed() {
        assert_eq!(
            judge_webrtc(&[], Some(ip("1.2.3.4"))),
            WebrtcVerdict::NoPublic
        );
        assert_eq!(judge_webrtc(&[], None), WebrtcVerdict::NoPublic);
    }

    #[test]
    fn webrtc_public_mismatching_egress_is_possible_leak() {
        // 任一公网 UDP 地址 ≠ HTTP 出口 → 可能泄漏（票面规则）
        assert_eq!(
            judge_webrtc(&[ip("203.0.113.7")], Some(ip("1.2.3.4"))),
            WebrtcVerdict::MaybeLeak
        );
        // 其余一致、仅一个不同也算
        assert_eq!(
            judge_webrtc(&[ip("1.2.3.4"), ip("203.0.113.7")], Some(ip("1.2.3.4"))),
            WebrtcVerdict::MaybeLeak
        );
        // IPv6 候选与 IPv4 出口天然不一致 → 泄漏
        assert_eq!(
            judge_webrtc(&[ip("1.2.3.4"), ip("2606:4700::1111")], Some(ip("1.2.3.4"))),
            WebrtcVerdict::MaybeLeak
        );
    }

    #[test]
    fn webrtc_public_matching_egress_is_clean() {
        assert_eq!(
            judge_webrtc(&[ip("1.2.3.4")], Some(ip("1.2.3.4"))),
            WebrtcVerdict::Clean
        );
        assert_eq!(
            judge_webrtc(&[ip("1.2.3.4"), ip("1.2.3.4")], Some(ip("1.2.3.4"))),
            WebrtcVerdict::Clean
        );
    }

    #[test]
    fn webrtc_unknown_egress_falls_back_to_distinct_count() {
        // 上游接口报告规则：多个不同公网 STUN IP → 可能泄漏
        assert_eq!(
            judge_webrtc(&[ip("203.0.113.7"), ip("203.0.113.9")], None),
            WebrtcVerdict::MaybeLeak
        );
        // 单一公网地址且出口未知：无法对照，不算泄漏
        assert_eq!(
            judge_webrtc(&[ip("203.0.113.7")], None),
            WebrtcVerdict::Clean
        );
    }
}
