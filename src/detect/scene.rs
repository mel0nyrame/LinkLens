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

/// 三个使用场景（TikTok / 社媒 / AI），各算一份 0-10 场景评分。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene {
    TikTok,
    Social,
    Ai,
}

impl Scene {
    /// 场景名（卡片行首）。
    pub fn name(self) -> &'static str {
        match self {
            Scene::TikTok => "TikTok",
            Scene::Social => "社媒",
            Scene::Ai => "AI",
        }
    }
}

/// 地区硬门槛结论。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gate {
    /// 无门槛，按公式正常计分。
    None,
    /// 地区被封锁：强制 0 分。
    Block,
    /// 部分可用：封顶 5 分。
    Partial,
}

impl Gate {
    /// 门槛说明文案（上游接口报告措辞）。
    pub fn label(self) -> &'static str {
        match self {
            Gate::None => "",
            Gate::Block => "地区不可用",
            Gate::Partial => "部分可用",
        }
    }
}

/// TikTok 场景封锁地区（报告 §3.3 REGION 表）。
pub const TIKTOK_BLOCK: [&str; 11] = [
    "cn", "hk", "in", "ir", "af", "kp", "jo", "so", "sn", "kg", "uz",
];
/// 社媒场景封锁地区。
pub const SOCIAL_BLOCK: [&str; 4] = ["cn", "ir", "kp", "tm"];
/// 社媒场景部分可用地区。
pub const SOCIAL_PARTIAL: [&str; 2] = ["ru", "mm"];
/// AI 场景封锁地区。
pub const AI_BLOCK: [&str; 8] = ["cn", "ru", "by", "ir", "kp", "cu", "sy", "af"];
/// AI 场景部分可用地区。
pub const AI_PARTIAL: [&str; 4] = ["hk", "mo", "ve", "mm"];

/// 地区硬门槛判定：查对应场景的 block/partial 表（大小写不敏感）。
pub fn gate_for(scene: Scene, country_code: &str) -> Gate {
    let cc = country_code.to_ascii_lowercase();
    let (block, partial): (&[&str], &[&str]) = match scene {
        Scene::TikTok => (&TIKTOK_BLOCK, &[]),
        Scene::Social => (&SOCIAL_BLOCK, &SOCIAL_PARTIAL),
        Scene::Ai => (&AI_BLOCK, &AI_PARTIAL),
    };
    if block.contains(&cc.as_str()) {
        Gate::Block
    } else if partial.contains(&cc.as_str()) {
        Gate::Partial
    } else {
        Gate::None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneScore {
    pub scene: Scene,
    pub score: u8,
    pub gate: Gate,
    pub verdict: &'static str,
    pub tip: &'static str,
}

pub fn scene_score(d: &crate::net::ip_score::Lookup, scene: Scene) -> Option<SceneScore> {
    // 上游接口报告 base 和最终场景分均四舍五入（Math.floor(x + 0.5)）。
    let mut score = (f64::from(d.risk.trust_score?) / 10.0 + 0.5).floor();
    let dc = d.risk.is_datacenter == Some(true)
        || d.risk.company_type == "hosting"
        || matches!(d.asn_kind.as_str(), "hosting" | "cdn");
    if ip_origin(&d.risk.country_code, &d.registered_country_code) == IpOrigin::Native {
        score += 0.5;
    }
    if !dc && d.risk.is_crawler != Some(true) && !d.is_public_service {
        score += 0.5;
    }
    if d.risk.company_type == "business" {
        score -= 0.5;
    }
    if ip_origin(&d.risk.country_code, &d.registered_country_code) == IpOrigin::Broadcast {
        score -= 1.0;
    }
    if dc {
        score -= if scene == Scene::Ai { 2.0 } else { 3.0 };
    }
    let pv =
        d.risk.is_proxy == Some(true) || d.risk.is_vpn == Some(true) || d.risk.is_tor == Some(true);
    if pv {
        score -= if scene == Scene::Ai { 2.0 } else { 3.0 };
    }
    if d.risk.is_abuser == Some(true) {
        score -= if scene == Scene::Ai { 0.5 } else { 1.0 };
    }
    if d.risk.is_crawler == Some(true) {
        score -= 0.5;
    }
    let raw = d
        .intelligence
        .get("abuser_score_raw")
        .filter(|v| !v.is_null())
        .unwrap_or(&d.abuser_score);
    let ab_pen = match parse_float(raw) {
        Some(v) if v > 0.05 => 2.0,
        Some(v) if v > 0.025 => 1.0,
        Some(v) if v > 0.01 => 0.5,
        Some(_) => 0.0,
        None => match d
            .intelligence
            .get("abuser_level")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "high" | "very_high" | "veryhigh" => 2.0,
            "elevated" => 1.0,
            _ => 0.0,
        },
    };
    let hp_raw = d
        .intelligence
        .get("rep_threat")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
        .or_else(|| d.intelligence.get("httpbl_threat"));
    let hp = hp_raw.and_then(number).unwrap_or(0.0);
    let hp_pen = if hp > 25.0 {
        2.0
    } else if hp > 0.0 {
        1.0
    } else {
        0.0
    };
    score -= ab_pen + hp_pen;
    let score = (score + 0.5).floor().clamp(0.0, 10.0) as u8;
    let risky = d.risk.is_abuser == Some(true)
        || ab_pen > 0.0
        || hp_pen > 0.0
        || d.risk.is_crawler == Some(true)
        || pv;
    let score = if risky { score.min(9) } else { score };
    Some(assess_score(
        scene,
        score,
        gate_for(scene, &d.risk.country_code),
    ))
}

// 对齐 JS parseFloat：上游 abuser_score 可带括号档位后缀，空串不是 0。
fn parse_float(value: &serde_json::Value) -> Option<f64> {
    if let Some(v) = value.as_f64() {
        return v.is_finite().then_some(v);
    }
    let s = value.as_str()?.trim_start();
    (1..=s.len())
        .rev()
        .filter(|&end| s.is_char_boundary(end))
        .find_map(|end| s[..end].parse::<f64>().ok().filter(|v| v.is_finite()))
}

fn number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|v| v.is_finite())
}

fn assess_score(scene: Scene, mut score: u8, gate: Gate) -> SceneScore {
    if gate == Gate::Block {
        score = 0;
    }
    if gate == Gate::Partial {
        score = score.min(5);
    }
    let verdict = match gate {
        Gate::Block => "地区不可用",
        Gate::Partial => "部分可用",
        Gate::None => match score {
            10 => "极佳",
            8..=9 => "推荐",
            5..=7 => "可用",
            _ => "不推荐",
        },
    };
    let tip = if gate == Gate::None && (5..8).contains(&score) {
        if scene == Scene::Ai {
            "GPT和Gemini可用，Claude不建议使用"
        } else {
            "仅可用，不代表推荐用于账户运营"
        }
    } else {
        ""
    };
    SceneScore {
        scene,
        score,
        gate,
        verdict,
        tip,
    }
}

/// 上游接口报告 Radar 值与缺省估算均结合本 IP 风险信号修正。
pub fn human_percent(d: &crate::net::ip_score::Lookup, radar: Option<f64>) -> (f64, bool) {
    let radar = radar.filter(|v| v.is_finite());
    let company_type = d.risk.company_type.to_ascii_lowercase();
    let asn_kind = d.asn_kind.to_ascii_lowercase();
    let dc = d.risk.is_datacenter == Some(true)
        || company_type == "hosting"
        || matches!(asn_kind.as_str(), "hosting" | "cdn");
    let mut h = radar.unwrap_or_else(|| {
        if d.is_public_service {
            2.0
        } else if d.risk.is_crawler == Some(true) {
            6.0
        } else if d.is_mobile || asn_kind == "mobile" {
            93.0
        } else if dc {
            18.0
        } else if company_type == "business" {
            65.0
        } else {
            88.0
        }
    });
    if d.risk.is_tor == Some(true) {
        h *= 0.4;
    } else if d.risk.is_proxy == Some(true) || d.risk.is_vpn == Some(true) {
        h *= 0.6;
    }
    if d.risk.is_crawler == Some(true) {
        h = h.min(h * 0.3 + 2.0);
    }
    if d.risk.is_abuser == Some(true) {
        h -= 5.0;
    }
    (h.clamp(1.0, 99.0), radar.is_none())
}

#[cfg(test)]
mod tests {
    use super::{Gate, IpOrigin, QueryKind, Scene, gate_for, ip_origin, validate_query_input};

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
            assert!(validate_query_input(bad).is_err(), "{bad} 不应通过校验");
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
    #[test]
    fn scene_region_gates_match_official_tables() {
        for (scene, blocked, partial) in [
            (
                Scene::TikTok,
                vec![
                    "CN", "HK", "IN", "IR", "AF", "KP", "JO", "SO", "SN", "KG", "UZ",
                ],
                vec![],
            ),
            (
                Scene::Social,
                vec!["CN", "IR", "KP", "TM"],
                vec!["RU", "MM"],
            ),
            (
                Scene::Ai,
                vec!["CN", "RU", "BY", "IR", "KP", "CU", "SY", "AF"],
                vec!["HK", "MO", "VE", "MM"],
            ),
        ] {
            for cc in blocked {
                assert_eq!(gate_for(scene, cc), Gate::Block, "{scene:?} {cc}");
            }
            for cc in partial {
                assert_eq!(gate_for(scene, &cc.to_lowercase()), Gate::Partial);
            }
            for cc in ["US", "JP", "SG", "TW", ""] {
                assert_eq!(gate_for(scene, cc), Gate::None);
            }
        }
    }

    #[test]
    fn google_public_dns_fixture_has_official_scene_scores() {
        let d = crate::net::ip_score::parse_lookup(include_str!(
            "../../tests/fixtures/ip-lookup-google.json"
        ))
        .unwrap();
        for (scene, expected, verdict) in [
            (Scene::TikTok, 4, "不推荐"),
            (Scene::Social, 4, "不推荐"),
            (Scene::Ai, 5, "可用"),
        ] {
            let score = super::scene_score(&d, scene).unwrap();
            assert_eq!(score.score, expected);
            assert_eq!(score.verdict, verdict);
        }
        assert_eq!(
            super::scene_score(&d, Scene::Ai).unwrap().tip,
            "GPT和Gemini可用，Claude不建议使用"
        );
    }

    fn fact(json: &str) -> crate::net::ip_score::Lookup {
        crate::net::ip_score::parse_lookup(json).unwrap()
    }

    #[test]
    fn scene_property_adjustments_match_site() {
        // trust=80 基线8；原生+0.5；人类+0.5；最终四舍五入。
        for (json, ai, social) in [
            (r#"{"trust_score":80,"is_public_service":true}"#, 8, 8),
            (
                r#"{"trust_score":80,"countryCode":"us","registered_country_code":"us","is_public_service":true}"#,
                9,
                9,
            ),
            (r#"{"trust_score":80}"#, 9, 9),
            (
                r#"{"trust_score":80,"countryCode":"us","registered_country_code":"jp"}"#,
                8,
                8,
            ),
            (
                r#"{"trust_score":80,"countryCode":"us","registered_country_code":"us","company_type":"business"}"#,
                9,
                9,
            ),
            (r#"{"trust_score":80,"is_datacenter":true}"#, 6, 5),
            (r#"{"trust_score":80,"company_type":"hosting"}"#, 6, 5),
            (r#"{"trust_score":80,"asn_kind":"cdn"}"#, 6, 5),
            (r#"{"trust_score":80,"asn_kind":"hosting"}"#, 6, 5),
        ] {
            let d = fact(json);
            assert_eq!(
                super::scene_score(&d, Scene::Ai).unwrap().score,
                ai,
                "{json}"
            );
            assert_eq!(
                super::scene_score(&d, Scene::Social).unwrap().score,
                social,
                "{json}"
            );
        }
    }

    #[test]
    fn risk_flags_are_deducted_once_with_scene_specific_penalties() {
        for (flags, ai, social) in [
            (r#""is_proxy":true"#, 7, 6),
            (r#""is_vpn":true"#, 7, 6),
            (r#""is_tor":true"#, 7, 6),
            (r#""is_proxy":true,"is_vpn":true,"is_tor":true"#, 7, 6),
            (r#""is_abuser":true"#, 8, 8),
            (r#""is_crawler":true"#, 8, 8),
        ] {
            let d = fact(&format!(r#"{{"trust_score":80,{flags}}}"#));
            assert_eq!(
                super::scene_score(&d, Scene::Ai).unwrap().score,
                ai,
                "{flags}"
            );
            assert_eq!(
                super::scene_score(&d, Scene::Social).unwrap().score,
                social,
                "{flags}"
            );
        }
    }

    #[test]
    fn abuse_thresholds_and_honeypot_fallback_follow_frontend() {
        for (intel, expected) in [
            (r#"{"abuser_score_raw":0.01}"#, 9),
            (r#"{"abuser_score_raw":0.0101}"#, 8),
            (r#"{"abuser_score_raw":0.025}"#, 8),
            (r#"{"abuser_score_raw":0.0251}"#, 8),
            (r#"{"abuser_score_raw":0.05}"#, 8),
            (r#"{"abuser_score_raw":0.0501}"#, 7),
            (r#"{"abuser_score_raw":"","abuser_level":"elevated"}"#, 8),
            (r#"{"abuser_level":"HIGH"}"#, 7),
            (r#"{"abuser_level":"very_high"}"#, 7),
            (r#"{"abuser_level":"veryhigh"}"#, 7),
            (r#"{"abuser_score_raw":0.001,"abuser_level":"high"}"#, 9),
            (r#"{"rep_threat":0}"#, 9),
            (r#"{"rep_threat":25}"#, 8),
            (r#"{"rep_threat":26}"#, 7),
            (r#"{"rep_threat":"","httpbl_threat":26}"#, 7),
        ] {
            let d = fact(&format!(r#"{{"trust_score":80,"intelligence":{intel}}}"#));
            assert_eq!(
                super::scene_score(&d, Scene::Ai).unwrap().score,
                expected,
                "{intel}"
            );
        }
        let d = fact(r#"{"trust_score":80,"abuser_score":"0.08 (High)"}"#);
        assert_eq!(super::scene_score(&d, Scene::Ai).unwrap().score, 7);
    }

    #[test]
    fn scene_clamping_risk_cap_and_region_gate_are_applied_last() {
        assert!(super::scene_score(&fact("{}"), Scene::Ai).is_none());
        let clean =
            fact(r#"{"trust_score":100,"countryCode":"us","registered_country_code":"us"}"#);
        assert_eq!(super::scene_score(&clean, Scene::Ai).unwrap().score, 10);
        let risky = fact(
            r#"{"trust_score":100,"countryCode":"us","registered_country_code":"us","is_abuser":true}"#,
        );
        assert_eq!(super::scene_score(&risky, Scene::Ai).unwrap().score, 9);
        assert_eq!(super::scene_score(&risky, Scene::Social).unwrap().score, 9);
        let worst =
            fact(r#"{"trust_score":0,"company_type":"hosting","is_tor":true,"is_abuser":true}"#);
        assert_eq!(super::scene_score(&worst, Scene::Ai).unwrap().score, 0);
        let block = fact(r#"{"trust_score":100,"countryCode":"CN"}"#);
        let result = super::scene_score(&block, Scene::Ai).unwrap();
        assert_eq!((result.score, result.verdict), (0, "地区不可用"));
        let partial = fact(r#"{"trust_score":100,"countryCode":"HK"}"#);
        let result = super::scene_score(&partial, Scene::Ai).unwrap();
        assert_eq!((result.score, result.verdict), (5, "部分可用"));
        let result = super::scene_score(&partial, Scene::Social).unwrap();
        assert_eq!((result.score, result.verdict), (10, "极佳"));
        let tiktok = super::scene_score(&partial, Scene::TikTok).unwrap();
        assert_eq!((tiktok.score, tiktok.verdict), (0, "地区不可用"));
    }

    #[test]
    fn radar_and_fallback_estimate_share_official_risk_adjustment() {
        assert_eq!(
            super::human_percent(&fact(r#"{"is_public_service":true}"#), None),
            (2.0, true)
        );
        assert_eq!(
            super::human_percent(&fact(r#"{"is_crawler":true}"#), None),
            (3.8, true)
        );
        assert_eq!(
            super::human_percent(&fact(r#"{"is_mobile":true}"#), None),
            (93.0, true)
        );
        assert_eq!(
            super::human_percent(&fact(r#"{"asn_kind":"cdn"}"#), None),
            (18.0, true)
        );
        assert_eq!(
            super::human_percent(&fact(r#"{"company_type":"business"}"#), None),
            (65.0, true)
        );
        assert_eq!(super::human_percent(&fact("{}"), None), (88.0, true));
        let d = fact(r#"{"is_tor":true,"is_proxy":true,"is_abuser":true}"#);
        assert_eq!(super::human_percent(&d, Some(50.0)), (15.0, false));
        assert_eq!(
            super::human_percent(&fact(r#"{"is_vpn":true}"#), Some(50.0)),
            (30.0, false)
        );
        assert_eq!(
            super::human_percent(&fact("{}"), Some(100.0)),
            (99.0, false)
        );
        assert_eq!(super::human_percent(&d, Some(2.66)), (1.0, false));
    }
    #[test]
    fn human_estimate_normalizes_network_type_case_independently_from_scene_scores() {
        for (json, h) in [
            (r#"{"company_type":"HOSTING"}"#, 18.0),
            (r#"{"asn_kind":"CDN"}"#, 18.0),
            (r#"{"asn_kind":"MOBILE"}"#, 93.0),
            (r#"{"company_type":"BUSINESS"}"#, 65.0),
        ] {
            assert_eq!(super::human_percent(&fact(json), None), (h, true));
        }
    }
}
