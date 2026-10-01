//! 归属地交叉校验：国内源中文归属地 vs 后端 geoip 国别码。
//!
//! 权威依据：net.coffee 接口报告 §3.1 与上游接口报告 `home-page.js` 的 `pickValidatedGeo`——
//! 国内源（ip138/my.ip.cn）的地区库对部分 APNIC 段陈旧，当其中文文本与
//! geoip 的权威国别码不一致时，以 `CC_TO_CN_NAME` + geoip 的 region/city/isp
//! 重拼中文归属地，来源标记「GeoIP（地区库纠正）」。

use super::geoip::GeoIp;

/// 约 50 国的中文前缀表（国别码 → 可认可的前缀写法）。
/// 逐项转录自上游接口报告 home-page.js 的 `CC_TO_CN_PREFIX`。
const CC_TO_CN_PREFIX: &[(&str, &[&str])] = &[
    ("cn", &["中国"]),
    ("hk", &["香港", "中国香港"]),
    ("mo", &["澳门", "中国澳门"]),
    ("tw", &["台湾", "中国台湾"]),
    ("jp", &["日本"]),
    ("kr", &["韩国"]),
    ("sg", &["新加坡"]),
    ("my", &["马来西亚"]),
    ("th", &["泰国"]),
    ("vn", &["越南"]),
    ("id", &["印度尼西亚", "印尼"]),
    ("ph", &["菲律宾"]),
    ("in", &["印度"]),
    ("pk", &["巴基斯坦"]),
    ("bd", &["孟加拉"]),
    ("ir", &["伊朗"]),
    ("il", &["以色列"]),
    ("ae", &["阿联酋", "阿拉伯联合酋长国"]),
    ("sa", &["沙特"]),
    ("tr", &["土耳其"]),
    ("us", &["美国"]),
    ("ca", &["加拿大"]),
    ("mx", &["墨西哥"]),
    ("br", &["巴西"]),
    ("ar", &["阿根廷"]),
    ("cl", &["智利"]),
    ("gb", &["英国"]),
    ("ie", &["爱尔兰"]),
    ("fr", &["法国"]),
    ("de", &["德国"]),
    ("it", &["意大利"]),
    ("es", &["西班牙"]),
    ("pt", &["葡萄牙"]),
    ("nl", &["荷兰"]),
    ("be", &["比利时"]),
    ("ch", &["瑞士"]),
    ("at", &["奥地利"]),
    ("se", &["瑞典"]),
    ("no", &["挪威"]),
    ("dk", &["丹麦"]),
    ("fi", &["芬兰"]),
    ("pl", &["波兰"]),
    ("cz", &["捷克"]),
    ("ru", &["俄罗斯", "俄国"]),
    ("ua", &["乌克兰"]),
    ("au", &["澳大利亚", "澳洲"]),
    ("nz", &["新西兰"]),
    ("za", &["南非"]),
    ("eg", &["埃及"]),
    ("ng", &["尼日利亚"]),
    ("ke", &["肯尼亚"]),
];

/// 国别码 → 标准中文国名（重拼归属地时使用）。
/// 逐项转录自上游接口报告 home-page.js 的 `CC_TO_CN_NAME`。
const CC_TO_CN_NAME: &[(&str, &str)] = &[
    ("cn", "中国"),
    ("hk", "中国香港"),
    ("mo", "中国澳门"),
    ("tw", "中国台湾"),
    ("jp", "日本"),
    ("kr", "韩国"),
    ("sg", "新加坡"),
    ("my", "马来西亚"),
    ("th", "泰国"),
    ("vn", "越南"),
    ("id", "印度尼西亚"),
    ("ph", "菲律宾"),
    ("in", "印度"),
    ("pk", "巴基斯坦"),
    ("bd", "孟加拉国"),
    ("ir", "伊朗"),
    ("il", "以色列"),
    ("ae", "阿联酋"),
    ("sa", "沙特"),
    ("tr", "土耳其"),
    ("us", "美国"),
    ("ca", "加拿大"),
    ("mx", "墨西哥"),
    ("br", "巴西"),
    ("ar", "阿根廷"),
    ("cl", "智利"),
    ("gb", "英国"),
    ("ie", "爱尔兰"),
    ("fr", "法国"),
    ("de", "德国"),
    ("it", "意大利"),
    ("es", "西班牙"),
    ("pt", "葡萄牙"),
    ("nl", "荷兰"),
    ("be", "比利时"),
    ("ch", "瑞士"),
    ("at", "奥地利"),
    ("se", "瑞典"),
    ("no", "挪威"),
    ("dk", "丹麦"),
    ("fi", "芬兰"),
    ("pl", "波兰"),
    ("cz", "捷克"),
    ("ru", "俄罗斯"),
    ("ua", "乌克兰"),
    ("au", "澳大利亚"),
    ("nz", "新西兰"),
    ("za", "南非"),
    ("eg", "埃及"),
    ("ng", "尼日利亚"),
    ("ke", "肯尼亚"),
];

/// 来源为「GeoIP（地区库纠正）」时的标注文案。
pub const CORRECTED_SOURCE: &str = "GeoIP（地区库纠正）";

/// 一个候选归属地：国内源给出的中文文本及其来源名。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GeoCandidate {
    pub geo: String,
    pub source: &'static str,
}

/// 交叉校验后的最终归属地与来源标注。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedGeo {
    pub geo: String,
    pub source: String,
}

/// 国别码对应的中文前缀列表；表外国别码为 `None`。
pub fn cn_prefixes(country_code: &str) -> Option<&'static [&'static str]> {
    CC_TO_CN_PREFIX
        .iter()
        .find(|(cc, _)| *cc == country_code)
        .map(|(_, prefixes)| *prefixes)
}

/// 国别码对应的标准中文国名；表外国别码为 `None`。
pub fn cn_name(country_code: &str) -> Option<&'static str> {
    CC_TO_CN_NAME
        .iter()
        .find(|(cc, _)| *cc == country_code)
        .map(|(_, name)| *name)
}

/// 校验 `text` 是否以某前缀开头且其后是边界（串尾、半角或全角空格）。
///
/// 边界要求避免「中国电信」这类运营商名误判为「中国」归属。
fn matches_prefix(prefixes: &[&str], text: &str) -> bool {
    prefixes.iter().any(|prefix| {
        text.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' ') || rest.starts_with('　'))
    })
}

/// 归属地交叉校验（上游接口报告 `pickValidatedGeo` 的移植）：
///
/// 1. geoip 国别码缺失：取最长的非空候选文本（并列取先出现者）；
/// 2. 有候选通过前缀校验：取通过者中最长者，来源保持国内源原名；
/// 3. 全部不通过：以标准中文国名 + geoip 的 region/city/isp 重拼，
///    来源标记「GeoIP（地区库纠正）」（region 与 city 相同时只保留其一）。
pub fn validate_geo(geoip: Option<&GeoIp>, candidates: &[GeoCandidate]) -> ResolvedGeo {
    let cc = geoip.map(|g| g.country_code.to_lowercase()).unwrap_or_default();

    if cc.is_empty() {
        let longest = candidates
            .iter()
            .filter(|c| !c.geo.is_empty())
            .max_by_key(|c| c.geo.chars().count());
        return match longest {
            Some(c) => ResolvedGeo { geo: c.geo.clone(), source: c.source.to_string() },
            None => candidates
                .first()
                .map(|c| ResolvedGeo { geo: c.geo.clone(), source: c.source.to_string() })
                .unwrap_or(ResolvedGeo { geo: String::new(), source: String::new() }),
        };
    }

    if let Some(prefixes) = cn_prefixes(&cc) {
        let valid = candidates.iter().filter(|c| !c.geo.is_empty() && matches_prefix(prefixes, &c.geo));
        if let Some(longest) = valid.max_by_key(|c| c.geo.chars().count()) {
            return ResolvedGeo { geo: longest.geo.clone(), source: longest.source.to_string() };
        }
    }

    rebuild_from_geoip(&cc, geoip)
}

/// 用标准中文国名 + geoip 的 region/city/isp 重拼中文归属地。
fn rebuild_from_geoip(_cc: &str, geoip: Option<&GeoIp>) -> ResolvedGeo {
    let geo = geoip.expect("重拼分支只在国别码非空时进入，必有 geoip");
    ResolvedGeo { geo: chinese_location(geo), source: CORRECTED_SOURCE.to_string() }
}

/// 仅凭 geoip 拼中文归属地：中文国名（表外用英文国名兜底）+ region/city/isp。
///
/// 供无国内源候选文本的卡（如 Cloudflare 出口）直接使用。
pub fn chinese_location(geoip: &GeoIp) -> String {
    let cc = geoip.country_code.to_lowercase();
    let mut parts = vec![cn_name(&cc).unwrap_or(geoip.country.as_str()).to_string()];
    if !geoip.region.is_empty() && geoip.region != geoip.city {
        parts.push(geoip.region.clone());
    }
    if !geoip.city.is_empty() {
        parts.push(geoip.city.clone());
    }
    if !geoip.isp.is_empty() {
        parts.push(geoip.isp.clone());
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::{CC_TO_CN_NAME, CC_TO_CN_PREFIX, GeoCandidate, ResolvedGeo, chinese_location, cn_name, cn_prefixes, matches_prefix, validate_geo};
    use crate::net::geoip::GeoIp;

    fn candidate(geo: &str, source: &'static str) -> GeoCandidate {
        GeoCandidate { geo: geo.to_string(), source }
    }

    fn geoip(cc: &str) -> GeoIp {
        GeoIp { country_code: cc.into(), ..GeoIp::default() }
    }

    #[test]
    fn prefix_table_covers_about_50_countries() {
        // 活站 JS 两表各 51 键（报告称「约 50 国」），逐项转录后应一致
        assert_eq!(CC_TO_CN_PREFIX.len(), 51);
        assert_eq!(CC_TO_CN_NAME.len(), 51);
        // 两表键一致
        for (cc, _) in CC_TO_CN_PREFIX {
            assert!(cn_name(cc).is_some(), "{cc} 应同时在前缀表与国名表中");
        }
    }

    #[test]
    fn lookup_tables_hit_common_cases() {
        assert_eq!(cn_prefixes("jp"), Some(&["日本"][..]));
        assert_eq!(cn_prefixes("id"), Some(&["印度尼西亚", "印尼"][..]));
        assert_eq!(cn_prefixes("zz"), None);
        assert_eq!(cn_name("bd"), Some("孟加拉国"));
        assert_eq!(cn_name("zz"), None);
    }

    #[test]
    fn prefix_match_requires_boundary() {
        assert!(matches_prefix(&["中国"], "中国 广东 珠海"));
        assert!(matches_prefix(&["中国"], "中国　广东"));
        assert!(matches_prefix(&["中国"], "中国"));
        assert!(!matches_prefix(&["中国"], "中国电信机房"));
        assert!(!matches_prefix(&["中国"], "中华人民共和国"));
    }

    #[test]
    fn matching_candidate_keeps_domestic_source() {
        let resolved = validate_geo(
            Some(&geoip("jp")),
            &[candidate("日本 东京", "iP138.com")],
        );
        assert_eq!(resolved.geo, "日本 东京");
        assert_eq!(resolved.source, "iP138.com");
    }

    #[test]
    fn mismatch_rebuilds_from_geoip_and_marks_correction() {
        let resolved = validate_geo(
            Some(&GeoIp {
                country_code: "us".into(),
                region: "California".into(),
                city: "Los Angeles".into(),
                isp: "NetLab Global".into(),
                ..GeoIp::default()
            }),
            &[candidate("中国 示例省份 示例城市  示例运营商", "IP.cn")],
        );
        assert_eq!(resolved.geo, "美国 California Los Angeles NetLab Global");
        assert_eq!(resolved.source, "GeoIP（地区库纠正）");
    }

    #[test]
    fn region_equal_to_city_is_deduplicated() {
        let resolved = validate_geo(
            Some(&GeoIp {
                country_code: "sg".into(),
                region: "Singapore".into(),
                city: "Singapore".into(),
                ..GeoIp::default()
            }),
            &[candidate("中国 上海", "iP138.com")],
        );
        assert_eq!(resolved.geo, "新加坡 Singapore");
    }

    #[test]
    fn unknown_country_code_falls_back_to_geoip_country() {
        let resolved = validate_geo(
            Some(&GeoIp { country: "Freedonia".into(), country_code: "zz".into(), ..GeoIp::default() }),
            &[candidate("中国 上海", "iP138.com")],
        );
        assert_eq!(resolved.geo, "Freedonia");
        assert_eq!(resolved.source, "GeoIP（地区库纠正）");
    }

    #[test]
    fn missing_geoip_keeps_longest_candidate() {
        let resolved = validate_geo(
            None,
            &[
                candidate("中国 广东", "iP138.com"),
                candidate("中国 示例省份 示例城市  示例运营商", "IP.cn"),
            ],
        );
        assert_eq!(resolved.geo, "中国 示例省份 示例城市  示例运营商");
        assert_eq!(resolved.source, "IP.cn");
    }

    #[test]
    fn same_ip_two_candidates_pick_longest_valid() {
        let resolved = validate_geo(
            Some(&geoip("cn")),
            &[
                candidate("中国", "iP138.com"),
                candidate("中国 示例省份 示例城市  示例运营商", "IP.cn"),
            ],
        );
        assert_eq!(resolved.geo, "中国 示例省份 示例城市  示例运营商");
        assert_eq!(resolved.source, "IP.cn");
    }

    #[test]
    fn no_candidates_with_geoip_rebuilds_alone() {
        let resolved = validate_geo(Some(&geoip("jp")), &[]);
        assert_eq!(resolved.geo, "日本");
        assert_eq!(
            resolved,
            ResolvedGeo { geo: "日本".into(), source: "GeoIP（地区库纠正）".into() }
        );
    }

    #[test]
    fn chinese_location_built_from_geoip_alone() {
        let geo = GeoIp {
            country: "United States".into(),
            country_code: "us".into(),
            region: "Virginia".into(),
            city: "Ashburn".into(),
            isp: "Amazon.com".into(),
        };
        assert_eq!(chinese_location(&geo), "美国 Virginia Ashburn Amazon.com");
    }

    #[test]
    fn chinese_location_outside_table_falls_back_to_country() {
        let geo = GeoIp { country: "Freedonia".into(), country_code: "zz".into(), ..GeoIp::default() };
        assert_eq!(chinese_location(&geo), "Freedonia");
    }

    #[test]
    fn chinese_location_region_equal_to_city_deduplicated() {
        let geo = GeoIp {
            country_code: "sg".into(),
            region: "Singapore".into(),
            city: "Singapore".into(),
            ..GeoIp::default()
        };
        assert_eq!(chinese_location(&geo), "新加坡 Singapore");
    }
}
