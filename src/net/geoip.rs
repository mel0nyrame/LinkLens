//! net.coffee 归属地缓存 API 客户端与展示辅助（国旗 emoji、归属地拼串）。
//!
//! 权威依据：net.coffee 接口报告 §2.1/§2.2 —— `/api/geoip/{ip}` 与
//! `/api/geoip-batch?ips=a,b`，字段 `country/region/city/isp/country_code`。

use std::collections::HashMap;

use serde::Deserialize;

/// net.coffee API 基址（上游后端服务）。
pub const API_BASE: &str = "https://ip.net.coffee";

/// 非法或缺失国别码时的兜底旗帜：地球 emoji。
const GLOBE: &str = "🌐";

/// 一条归属地记录；`country_code` 为小写 ISO-2。
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
pub struct GeoIp {
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub isp: String,
    #[serde(default)]
    pub country_code: String,
}

impl GeoIp {
    /// 中文界面外的原始归属地拼串：`[country, region, city, isp]` 去空后以空格相连。
    pub fn geo_string(&self) -> String {
        [self.country.as_str(), self.region.as_str(), self.city.as_str(), self.isp.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 国旗 emoji：由小写 ISO-2 国别码构造区域指示符；空码或非法码返回地球 emoji。
pub fn flag_emoji(country_code: &str) -> String {
    let lower = country_code.to_lowercase();
    if lower.len() != 2 || !lower.chars().all(|c| c.is_ascii_lowercase()) {
        return GLOBE.to_string();
    }
    lower
        .chars()
        .map(|c| char::from_u32(0x1f1e6 + u32::from(c) - u32::from('a')).expect("a-z 邻域无空洞"))
        .collect()
}

/// 单查归属地；网络失败或响应不合法为 `None`。
pub async fn fetch_geoip(client: &reqwest::Client, ip: &str) -> Option<GeoIp> {
    client
        .get(format!("{API_BASE}/api/geoip/{ip}"))
        .send()
        .await
        .ok()?
        .json::<GeoIp>()
        .await
        .ok()
}

/// 批量归属地：一次请求补齐全部出口 IP 的国旗；失败为 `None`。
pub async fn fetch_geoip_batch(
    client: &reqwest::Client,
    ips: &[String],
) -> Option<HashMap<String, GeoIp>> {
    if ips.is_empty() {
        return Some(HashMap::new());
    }
    client
        .get(format!("{API_BASE}/api/geoip-batch"))
        .query(&[("ips", ips.join(","))])
        .send()
        .await
        .ok()?
        .json::<HashMap<String, GeoIp>>()
        .await
        .ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{GeoIp, flag_emoji};

    /// 报告 §2.2 的 geoip-batch 真实响应样例。
    const BATCH_JSON: &str = r#"{"8.8.8.8": {"country": "United States", "region": "", "city": "", "isp": "Google", "country_code": "us"},
 "1.1.1.1": {"country": "Australia", "region": "Queensland", "city": "South Brisbane", "isp": "Cloudflare, Inc.", "country_code": "au"},
 "2606:4700:4700::1111": {"country": "United States", "region": "California", "city": "San Francisco", "isp": "Cloudflare, Inc.", "country_code": "us"}}"#;

    #[test]
    fn batch_sample_deserializes() {
        let map: HashMap<String, GeoIp> = serde_json::from_str(BATCH_JSON).expect("样例应可反序列化");
        assert_eq!(map.len(), 3);
        let cloudflare = &map["1.1.1.1"];
        assert_eq!(cloudflare.country_code, "au");
        assert_eq!(cloudflare.city, "South Brisbane");
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let geo: GeoIp = serde_json::from_str(r#"{"country": "United States", "country_code": "us"}"#)
            .expect("缺字段应容忍");
        assert_eq!(geo.region, "");
        assert_eq!(geo.isp, "");
    }

    #[test]
    fn geo_string_skips_empty_parts() {
        let geo = GeoIp {
            country: "United States".into(),
            city: "San Francisco".into(),
            ..GeoIp::default()
        };
        assert_eq!(geo.geo_string(), "United States San Francisco");
        assert_eq!(GeoIp::default().geo_string(), "");
    }

    #[test]
    fn flag_emoji_from_iso_codes() {
        assert_eq!(flag_emoji("cn"), "\u{1f1e8}\u{1f1f3}");
        assert_eq!(flag_emoji("us"), "\u{1f1fa}\u{1f1f8}");
        assert_eq!(flag_emoji("jp"), "\u{1f1ef}\u{1f1f5}");
    }

    #[test]
    fn flag_emoji_accepts_uppercase() {
        assert_eq!(flag_emoji("HK"), flag_emoji("hk"));
    }

    #[test]
    fn flag_emoji_falls_back_to_globe() {
        assert_eq!(flag_emoji(""), "🌐");
        assert_eq!(flag_emoji("xyz"), "🌐");
    }
}
