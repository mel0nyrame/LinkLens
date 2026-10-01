//! IP 评分页的判定纯逻辑：输入校验、原生/广播推导、三场景评分。
//!
//! 权威依据：net.coffee 接口报告 §3.3（场景评分公式与地区硬门槛按接口报告定义实现）。
//! 测试接缝：全部为纯函数，报告真实样例直接作 fixture。

/// 查询目标的地址族。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QueryKind {
    /// IPv4（支持全部 v2 区块）。
    V4,
    /// IPv6（heat/DNSBL 不支持，对应区块隐藏）。
    V6,
}

/// 校验通过后的查询目标：规范化 IP 文本 + 地址族。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QueryTarget {
    pub ip: String,
    pub kind: QueryKind,
}

/// 输入校验：仅接受合法 IPv4/IPv6（上游接口报告搜索框同规则），
/// 返回规范化后的目标；非法输入给明确中文报错。
pub fn validate_query_input(text: &str) -> Result<QueryTarget, &'static str> {
    let trimmed = text.trim();
    let addr: std::net::IpAddr = trimmed
        .parse()
        .map_err(|_| "请输入合法的 IPv4 或 IPv6 地址")?;
    let kind = match addr {
        std::net::IpAddr::V4(_) => QueryKind::V4,
        std::net::IpAddr::V6(_) => QueryKind::V6,
    };
    Ok(QueryTarget {
        // `IpAddr::to_string` 即规范化形式（IPv6 压缩零段、统一小写）
        ip: addr.to_string(),
        kind,
    })
}

/// 原生/广播 IP 推导结果。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IpOrigin {
    /// 注册国与归属国一致。
    Native,
    /// 注册国与归属国不一致（广播 IP）。
    Broadcast,
    /// 任一国别码缺失，无从判定。
    Unknown,
}

/// 原生/广播推导：`countryCode`（归属国）vs `registered_country_code`（注册国），
/// 大小写不敏感；任一缺失为 `Unknown`（不渲染徽章）。
pub fn ip_origin(country_code: &str, registered_country_code: &str) -> IpOrigin {
    let home = country_code.trim();
    let registered = registered_country_code.trim();
    if home.is_empty() || registered.is_empty() {
        return IpOrigin::Unknown;
    }
    if home.eq_ignore_ascii_case(registered) {
        IpOrigin::Native
    } else {
        IpOrigin::Broadcast
    }
}

#[cfg(test)]
mod tests {
    use super::{IpOrigin, QueryKind, ip_origin, validate_query_input};

    #[test]
    fn accepts_valid_ipv4() {
        let target = validate_query_input("8.8.8.8").expect("合法 IPv4 应通过");
        assert_eq!(target.ip, "8.8.8.8");
        assert_eq!(target.kind, QueryKind::V4);
    }

    #[test]
    fn accepts_valid_ipv6_and_canonicalizes() {
        let target = validate_query_input(" 2606:4700:4700:0000::1111 ").expect("合法 IPv6 应通过");
        assert_eq!(target.ip, "2606:4700:4700::1111");
        assert_eq!(target.kind, QueryKind::V6);
    }

    #[test]
    fn rejects_hostname_and_garbage() {
        for bad in ["claude.ai", "hello world", "1.2.3.4.5"] {
            let err = validate_query_input(bad).expect_err("非法输入应被拒绝");
            assert!(err.contains("IPv4"), "{bad} 的报错应说明合法格式：{err}");
        }
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(validate_query_input("").is_err());
        assert!(validate_query_input("   ").is_err());
    }

    #[test]
    fn rejects_cidr_port_and_malformed() {
        for bad in ["8.8.8.0/24", "8.8.8.8:53", "1.2.3", "::99999"] {
            assert!(
                validate_query_input(bad).is_err(),
                "{bad} 不应通过校验"
            );
        }
    }

    #[test]
    fn native_when_registered_matches_country() {
        assert_eq!(ip_origin("us", "us"), IpOrigin::Native);
        assert_eq!(ip_origin("US", "us"), IpOrigin::Native);
    }

    #[test]
    fn broadcast_when_registered_differs() {
        assert_eq!(ip_origin("jp", "us"), IpOrigin::Broadcast);
    }

    #[test]
    fn unknown_when_either_code_missing() {
        assert_eq!(ip_origin("", "us"), IpOrigin::Unknown);
        assert_eq!(ip_origin("jp", ""), IpOrigin::Unknown);
        assert_eq!(ip_origin("  ", "  "), IpOrigin::Unknown);
    }
}
