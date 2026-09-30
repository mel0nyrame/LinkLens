//! IP 打码与紧凑显示：隐藏 IP 开关的纯逻辑。
//!
//! 依据接口报告实现 `maskIpText`/`compactIp`：
//! - IPv4 `1.2.3.4` → `1.2.*.*`（保留前两段）；
//! - IPv6 保留前两个 hextet 后接 `:*`；
//! - 紧凑 IPv6（`2001:db8…a6c`）→ `2001:db8…*`；
//! - 其余文本原样返回。
//!
//! 仅影响显示，不影响数据；非 IP 文本不做替换。

/// 隐藏开关开启时的 IP 打码显示。
pub fn mask_ip(ip: &str) -> String {
    let ip = ip.trim();
    if is_ipv4(ip) {
        let mut parts = ip.split('.');
        let first = parts.next().unwrap_or("*");
        let second = parts.next().unwrap_or("*");
        return format!("{first}.{second}.*.*");
    }
    if let Some(idx) = ip.find('…') {
        // 保留首段与省略号，尾部整体打码
        return format!("{}*", &ip[..idx + '…'.len_utf8()]);
    }
    if ip.contains(':') {
        let mut parts = ip.split(':');
        let first = parts.next().unwrap_or("");
        let second = parts.next().unwrap_or("");
        return format!("{first}:{second}:*");
    }
    ip.to_string()
}

/// 界面显示用的 IP 文本：`hide` 时打码，否则原样。
pub fn display_ip(ip: &str, hide: bool) -> String {
    if hide { mask_ip(ip) } else { ip.to_string() }
}

/// 长 IPv6 的紧凑显示：`2001:db8::…:a6c` → `2001:db8…a6c`（≤22 字符原样）。
pub fn compact_ipv6(ip: &str) -> String {
    let ip = ip.trim();
    if !ip.contains(':') || ip.chars().count() <= 22 {
        return ip.to_string();
    }
    let mut parts = ip.split(':');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    let last = ip.rsplit(':').next().unwrap_or_default();
    format!("{first}:{second}…{last}")
}

/// 纯 IPv4 形式（各段数字，四个点分段）。
fn is_ipv4(text: &str) -> bool {
    let octets: Vec<&str> = text.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|o| !o.is_empty() && o.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::{compact_ipv6, display_ip, mask_ip};

    #[test]
    fn ipv4_keeps_first_two_octets() {
        assert_eq!(mask_ip("192.0.2.216"), "192.0.*.*");
        assert_eq!(mask_ip("1.2.3.4"), "1.2.*.*");
    }

    #[test]
    fn ipv6_keeps_first_two_hextets() {
        assert_eq!(mask_ip("2606:4700:4700::1111"), "2606:4700:*");
    }

    #[test]
    fn compact_ipv6_form_is_masked_after_ellipsis() {
        assert_eq!(mask_ip("2001:db8…a6c"), "2001:db8…*");
    }

    #[test]
    fn non_ip_text_passes_through() {
        assert_eq!(mask_ip("获取失败"), "获取失败");
        assert_eq!(mask_ip(""), "");
    }

    #[test]
    fn display_ip_switches_on_flag() {
        assert_eq!(display_ip("1.2.3.4", true), "1.2.*.*");
        assert_eq!(display_ip("1.2.3.4", false), "1.2.3.4");
    }

    #[test]
    fn compact_ipv6_keeps_short_addresses() {
        assert_eq!(compact_ipv6("::1"), "::1");
        assert_eq!(compact_ipv6("2606:4700:4700::1111"), "2606:4700:4700::1111");
    }

    #[test]
    fn compact_ipv6_trims_long_addresses() {
        let long = "2001:db8:1234:5678:90ab:cdef:1111:2222";
        assert_eq!(compact_ipv6(long), "2001:db8…2222");
    }

    #[test]
    fn compact_ipv4_is_untouched() {
        assert_eq!(compact_ipv6("192.0.2.216"), "192.0.2.216");
    }
}
