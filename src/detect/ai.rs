//! Claude/GPT 出口检测的判定纯逻辑。
//!
//! 权威依据：net.coffee 接口报告 §3.2（Claude）与 §3.4（GPT，与 Claude 完全同构）。
//! 受限地区表与信任分档位按接口报告定义实现；命中受限地区时信任分强制按 0 处理。

use crate::net::geoip::GeoIp;
use crate::net::iprisk::Iprisk;

/// 受限地区码表（小写 ISO-2）。上游接口报告 `CLAUDE_RESTRICTED_CC` 与
/// `OPENAI_RESTRICTED_CC` 完全相同的 10 个地区。
pub const RESTRICTED_CC: [&str; 10] = ["cn", "hk", "mo", "ru", "kp", "ir", "sy", "cu", "by", "ve"];

/// 出口国别码是否命中受限地区（大小写不敏感）。
pub fn is_restricted(country_code: &str) -> bool {
    let lower = country_code.to_lowercase();
    RESTRICTED_CC.contains(&lower.as_str())
}

/// 信任分档位（上游接口报告分档：≥95 极度纯净 / ≥80 纯净 / ≥50 良好 / ≥25 中性 / <25 可疑）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrustTier {
    /// ≥95。
    ExtremelyPure,
    /// ≥80。
    Pure,
    /// ≥50。
    Good,
    /// ≥25。
    Neutral,
    /// <25。
    Suspicious,
}

impl TrustTier {
    /// 档位徽章文案。
    pub fn label(self) -> &'static str {
        match self {
            TrustTier::ExtremelyPure => "极度纯净",
            TrustTier::Pure => "纯净",
            TrustTier::Good => "良好",
            TrustTier::Neutral => "中性",
            TrustTier::Suspicious => "可疑",
        }
    }
}

/// 信任分档位判定；分值超过 100 按 100 处理。
pub fn trust_tier(score: u8) -> TrustTier {
    match score.min(100) {
        95..=100 => TrustTier::ExtremelyPure,
        80..=94 => TrustTier::Pure,
        50..=79 => TrustTier::Good,
        25..=49 => TrustTier::Neutral,
        _ => TrustTier::Suspicious,
    }
}

/// 受限覆盖后的信任分：命中受限地区一律按 0；风险库缺失时保持 `None`（受限除外）。
pub fn effective_score(raw: Option<u8>, restricted: bool) -> Option<u8> {
    if restricted {
        return Some(0);
    }
    raw
}

/// 可用性时延分档（上游接口报告 `detectClaudeAvail`：<250ms 正常 / <500ms 良好 / 其余较慢 / 异常不可访问）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AvailTier {
    /// <250ms。
    Normal,
    /// <500ms。
    Good,
    /// 其余可达情况。
    Slow,
    /// 探测失败 / 超时。
    Unreachable,
}

impl AvailTier {
    /// 档位文案。
    pub fn label(self) -> &'static str {
        match self {
            AvailTier::Normal => "正常",
            AvailTier::Good => "良好",
            AvailTier::Slow => "较慢",
            AvailTier::Unreachable => "不可访问",
        }
    }
}

/// 可用性时延分档判定；`None` 一律不可访问。
pub fn availability_tier(latency: Option<u64>) -> AvailTier {
    match latency {
        None => AvailTier::Unreachable,
        Some(ms) if ms < 250 => AvailTier::Normal,
        Some(ms) if ms < 500 => AvailTier::Good,
        Some(_) => AvailTier::Slow,
    }
}

/// 多目标可用性探测的最优（最小成功）时延；全部失败为 `None`。
pub fn best_latency(latencies: &[Option<u64>]) -> Option<u64> {
    latencies.iter().flatten().copied().min()
}

/// 安全检测逐项：`（项目名, 是否命中风险）`，顺序固定：VPN/代理/Tor/机器人/滥用记录。
pub fn security_items(risk: &Iprisk) -> Vec<(&'static str, bool)> {
    vec![
        ("VPN", risk.is_vpn == Some(true)),
        ("代理", risk.is_proxy == Some(true)),
        ("Tor", risk.is_tor == Some(true)),
        ("机器人", risk.is_crawler == Some(true)),
        ("滥用记录", risk.is_abuser == Some(true)),
    ]
}

/// 出口国别码的解析结果。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExitCountry {
    /// 小写 ISO-2 国别码。
    pub code: String,
    /// `true` 表示 geoip 缺失、由 trace `loc=` 兜底（IPv6 出口常见）。
    pub from_trace_fallback: bool,
}

/// 出口国别码解析：geoip 优先；geoip 缺失或国别码为空时用 trace `loc=` 兜底。
pub fn resolve_exit_country(geo: Option<&GeoIp>, trace_loc: Option<&str>) -> Option<ExitCountry> {
    if let Some(code) = geo
        .map(|g| g.country_code.trim().to_lowercase())
        .filter(|code| !code.is_empty())
    {
        return Some(ExitCountry {
            code,
            from_trace_fallback: false,
        });
    }
    let loc = trace_loc?.trim().to_lowercase();
    if loc.is_empty() {
        return None;
    }
    Some(ExitCountry {
        code: loc,
        from_trace_fallback: true,
    })
}

/// trace `loc=` 兜底时的中文国名（内置国别码映射，复用 net 层中文国名表）；表外为空串。
pub fn fallback_location_text(code: &str) -> String {
    crate::net::cc::cn_name(&code.to_lowercase())
        .unwrap_or("")
        .to_string()
}

/// 服务状态级别（status.json 的 `overall_indicator`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusLevel {
    /// none：全部服务正常。
    AllNormal,
    /// minor：轻微故障。
    Minor,
    /// major：重大故障。
    Major,
    /// critical：严重故障。
    Critical,
    /// maintenance：维护中。
    Maintenance,
}

/// `overall_indicator` 到级别的映射；未知取值为 `None`（界面按原始文本弱化显示）。
pub fn status_level(indicator: &str) -> Option<StatusLevel> {
    match indicator.to_lowercase().as_str() {
        "none" => Some(StatusLevel::AllNormal),
        "minor" => Some(StatusLevel::Minor),
        "major" => Some(StatusLevel::Major),
        "critical" => Some(StatusLevel::Critical),
        "maintenance" => Some(StatusLevel::Maintenance),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AvailTier, ExitCountry, RESTRICTED_CC, StatusLevel, TrustTier, availability_tier,
        best_latency, effective_score, fallback_location_text, is_restricted, resolve_exit_country,
        security_items, status_level, trust_tier,
    };
    use crate::net::geoip::GeoIp;
    use crate::net::iprisk::Iprisk;

    /// 报告 §2.3 样例的安全信号取值（全 false）。
    fn clean_risk() -> Iprisk {
        Iprisk {
            is_vpn: Some(false),
            is_proxy: Some(false),
            is_tor: Some(false),
            is_crawler: Some(false),
            is_abuser: Some(false),
            ..Iprisk::default()
        }
    }

    #[test]
    fn restricted_table_is_the_ten_official_regions() {
        assert_eq!(
            RESTRICTED_CC,
            ["cn", "hk", "mo", "ru", "kp", "ir", "sy", "cu", "by", "ve"]
        );
    }

    #[test]
    fn restricted_codes_hit_case_insensitively() {
        for cc in ["cn", "hk", "mo", "ru", "kp", "ir", "sy", "cu", "by", "ve"] {
            assert!(is_restricted(cc), "{cc} 应命中受限地区");
            assert!(is_restricted(&cc.to_uppercase()), "{cc} 大写应命中");
        }
    }

    #[test]
    fn common_proxy_exits_are_not_restricted() {
        // 报告 §3.4：TW/KR/SG/JP/UA 等常见代理出口不在受限表
        for cc in ["us", "jp", "tw", "kr", "sg", "de", "ua", ""] {
            assert!(!is_restricted(cc), "{cc} 不应命中受限地区");
        }
    }

    #[test]
    fn trust_tier_thresholds_match_site() {
        assert_eq!(trust_tier(100), TrustTier::ExtremelyPure);
        assert_eq!(trust_tier(95), TrustTier::ExtremelyPure);
        assert_eq!(trust_tier(94), TrustTier::Pure);
        assert_eq!(trust_tier(80), TrustTier::Pure);
        assert_eq!(trust_tier(79), TrustTier::Good);
        assert_eq!(trust_tier(50), TrustTier::Good);
        assert_eq!(trust_tier(49), TrustTier::Neutral);
        assert_eq!(trust_tier(25), TrustTier::Neutral);
        assert_eq!(trust_tier(24), TrustTier::Suspicious);
        assert_eq!(trust_tier(0), TrustTier::Suspicious);
    }

    #[test]
    fn trust_tier_clamps_above_hundred() {
        assert_eq!(trust_tier(255), TrustTier::ExtremelyPure);
    }

    #[test]
    fn tier_labels_are_official_wording() {
        assert_eq!(TrustTier::ExtremelyPure.label(), "极度纯净");
        assert_eq!(TrustTier::Pure.label(), "纯净");
        assert_eq!(TrustTier::Good.label(), "良好");
        assert_eq!(TrustTier::Neutral.label(), "中性");
        assert_eq!(TrustTier::Suspicious.label(), "可疑");
    }

    #[test]
    fn effective_score_passes_through_when_not_restricted() {
        assert_eq!(effective_score(Some(85), false), Some(85));
        assert_eq!(effective_score(Some(0), false), Some(0));
        assert_eq!(effective_score(None, false), None);
    }

    #[test]
    fn restricted_forces_zero_even_without_iprisk() {
        assert_eq!(effective_score(Some(85), true), Some(0));
        assert_eq!(effective_score(None, true), Some(0));
    }

    #[test]
    fn availability_tier_buckets_match_site() {
        assert_eq!(availability_tier(Some(0)), AvailTier::Normal);
        assert_eq!(availability_tier(Some(249)), AvailTier::Normal);
        assert_eq!(availability_tier(Some(250)), AvailTier::Good);
        assert_eq!(availability_tier(Some(499)), AvailTier::Good);
        assert_eq!(availability_tier(Some(500)), AvailTier::Slow);
        assert_eq!(availability_tier(Some(8000)), AvailTier::Slow);
        assert_eq!(availability_tier(None), AvailTier::Unreachable);
    }

    #[test]
    fn availability_tier_labels_are_official_wording() {
        assert_eq!(AvailTier::Normal.label(), "正常");
        assert_eq!(AvailTier::Good.label(), "良好");
        assert_eq!(AvailTier::Slow.label(), "较慢");
        assert_eq!(AvailTier::Unreachable.label(), "不可访问");
    }

    #[test]
    fn best_latency_takes_minimum_success() {
        assert_eq!(
            best_latency(&[Some(300), None, Some(180), Some(260)]),
            Some(180)
        );
        assert_eq!(best_latency(&[None, None]), None);
        assert_eq!(best_latency(&[]), None);
        assert_eq!(best_latency(&[Some(42)]), Some(42));
    }

    #[test]
    fn security_items_cover_five_signals_in_order() {
        let items = security_items(&clean_risk());
        assert_eq!(
            items,
            vec![
                ("VPN", false),
                ("代理", false),
                ("Tor", false),
                ("机器人", false),
                ("滥用记录", false),
            ]
        );
    }

    #[test]
    fn security_items_flag_true_signals() {
        let risk = Iprisk {
            is_vpn: Some(true),
            is_proxy: Some(true),
            is_tor: Some(true),
            is_crawler: Some(true),
            is_abuser: Some(true),
            ..Iprisk::default()
        };
        assert!(security_items(&risk).iter().all(|(_, hit)| *hit));
    }

    #[test]
    fn missing_safety_fields_are_not_flagged() {
        let items = security_items(&Iprisk::default());
        assert!(items.iter().all(|(_, hit)| !hit));
    }

    #[test]
    fn exit_country_prefers_geoip() {
        let geo = GeoIp {
            country_code: "us".into(),
            ..GeoIp::default()
        };
        assert_eq!(
            resolve_exit_country(Some(&geo), Some("jp")),
            Some(ExitCountry {
                code: "us".into(),
                from_trace_fallback: false,
            })
        );
    }

    #[test]
    fn ipv6_without_geo_falls_back_to_trace_loc() {
        assert_eq!(
            resolve_exit_country(None, Some("JP")),
            Some(ExitCountry {
                code: "jp".into(),
                from_trace_fallback: true,
            })
        );
        // geo 存在但国别码为空同样兜底
        let empty = GeoIp::default();
        assert_eq!(
            resolve_exit_country(Some(&empty), Some("de")),
            Some(ExitCountry {
                code: "de".into(),
                from_trace_fallback: true,
            })
        );
    }

    #[test]
    fn exit_country_none_when_no_sources() {
        assert_eq!(resolve_exit_country(None, None), None);
        assert_eq!(resolve_exit_country(None, Some("")), None);
    }

    #[test]
    fn fallback_location_uses_builtin_country_names() {
        assert_eq!(fallback_location_text("jp"), "日本");
        assert_eq!(fallback_location_text("US"), "美国");
        // 表外国别码只给空串，界面以旗 + 码呈现
        assert_eq!(fallback_location_text("zz"), "");
        assert_eq!(fallback_location_text(""), "");
    }

    #[test]
    fn status_levels_map_official_indicators() {
        assert_eq!(status_level("none"), Some(StatusLevel::AllNormal));
        assert_eq!(status_level("minor"), Some(StatusLevel::Minor));
        assert_eq!(status_level("major"), Some(StatusLevel::Major));
        assert_eq!(status_level("critical"), Some(StatusLevel::Critical));
        assert_eq!(status_level("maintenance"), Some(StatusLevel::Maintenance));
        assert_eq!(status_level("unknown"), None);
        assert_eq!(status_level(""), None);
    }
}
