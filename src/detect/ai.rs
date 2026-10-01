//! Claude/GPT 出口检测的判定纯逻辑。
//!
//! 权威依据：net.coffee 接口报告 §3.2（Claude）与 §3.4（GPT，与 Claude 完全同构）。
//! 受限地区表与信任分档位按接口报告定义实现；命中受限地区时信任分强制按 0 处理。

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

#[cfg(test)]
mod tests {
    use super::{RESTRICTED_CC, TrustTier, effective_score, is_restricted, trust_tier};

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
}
