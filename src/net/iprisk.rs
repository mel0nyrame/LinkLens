//! net.coffee `/api/iprisk/{ip}` 客户端与 IP 属性徽章推导。
//!
//! 权威依据：net.coffee 接口报告 §2.3。注意陷阱：后端按 CIDR 段（IPv4 /24、IPv6 /48）
//! 聚合缓存，响应里的 `ip` 可能是段代表 IP 而非请求 IP，界面需如实标注。

use serde::Deserialize;

use super::geoip::API_BASE;

/// 一条 IP 风险/属性记录（字段按需取用，camelCase 字段用 rename 对齐）。
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
pub struct Iprisk {
    /// 响应自身携带的 IP，可能是 /24 段代表 IP 而非请求 IP。
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub cidr: String,
    #[serde(rename = "is_datacenter", default)]
    pub is_datacenter: Option<bool>,
    #[serde(rename = "isResidential", default)]
    pub is_residential: Option<bool>,
    #[serde(rename = "company_type", default)]
    pub company_type: String,
    #[serde(rename = "company_name", default)]
    pub company_name: String,
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(rename = "asOrganization", default)]
    pub as_organization: String,
    #[serde(rename = "countryCode", default)]
    pub country_code: String,
}

impl Iprisk {
    /// IP 属性徽章：家庭宽带 / 机房IP / 商业专线 / 教育（商业宽带为家宽的商务变体）。
    ///
    /// 判定阶梯采用上游接口报告首页；无从判定时为 `None`（界面不渲染徽章）。
    pub fn property_badge(&self) -> Option<&'static str> {
        let residential = self.is_residential == Some(true);
        let datacenter = self.is_datacenter == Some(true);
        match self.company_type.as_str() {
            "business" if residential => Some("商业宽带"),
            "business" if datacenter => Some("商业专线"),
            _ if residential => Some("家庭宽带"),
            _ if datacenter => Some("机房IP"),
            "education" => Some("教育"),
            _ => None,
        }
    }
}

/// 查询 IP 风险/属性；网络失败或响应不合法为 `None`。
pub async fn fetch_iprisk(client: &reqwest::Client, ip: &str) -> Option<Iprisk> {
    client
        .get(format!("{API_BASE}/api/iprisk/{ip}"))
        .send()
        .await
        .ok()?
        .json::<Iprisk>()
        .await
        .ok()
}

#[cfg(test)]
mod tests {
    use super::Iprisk;

    /// 报告 §2.3 脱敏样例（请求 203.0.113.179，返回段代表 203.0.113.12），字段节选。
    const SAMPLE_JSON: &str = r#"{"ip": "203.0.113.12", "cidr": "203.0.113.0/24", "is_datacenter": true, "isResidential": false, "isBroadcast": null,
 "is_vpn": false, "is_proxy": false, "is_tor": false, "is_crawler": false, "is_abuser": false, "is_mobile": false,
 "company_type": "hosting", "company_name": "Clean Pipe Networks LLC", "abuser_score": "0 (Very Low)",
 "datacenter_name": "Clean Pipe Networks LLC", "asn": 979, "asOrganization": "NetLab Global",
 "country": "United States", "countryCode": "us", "region": "California", "city": "Los Angeles",
 "timezone": "America/Los_Angeles", "src": "g1", "rdns": "", "rpki_status": "valid", "trust_score": 85}"#;

    #[test]
    fn sample_deserializes_with_camel_case_fields() {
        let risk: Iprisk = serde_json::from_str(SAMPLE_JSON).expect("样例应可反序列化");
        assert_eq!(risk.ip, "203.0.113.12");
        assert_eq!(risk.cidr, "203.0.113.0/24");
        assert_eq!(risk.is_datacenter, Some(true));
        assert_eq!(risk.is_residential, Some(false));
        assert_eq!(risk.company_type, "hosting");
        assert_eq!(risk.as_organization, "NetLab Global");
        assert_eq!(risk.country_code, "us");
        // 样例里的额外字段（isBroadcast/trust_score 等）不进结构体也不报错
    }

    #[test]
    fn datacenter_gets_machine_room_badge() {
        let risk = Iprisk { is_datacenter: Some(true), company_type: "hosting".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), Some("机房IP"));
    }

    #[test]
    fn residential_gets_home_broadband_badge() {
        let risk = Iprisk { is_residential: Some(true), company_type: "isp".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), Some("家庭宽带"));
    }

    #[test]
    fn datacenter_business_gets_dedicated_line_badge() {
        let risk = Iprisk { is_datacenter: Some(true), company_type: "business".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), Some("商业专线"));
    }

    #[test]
    fn residential_business_gets_business_broadband_badge() {
        let risk = Iprisk { is_residential: Some(true), company_type: "business".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), Some("商业宽带"));
    }

    #[test]
    fn education_type_gets_education_badge() {
        let risk = Iprisk { company_type: "education".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), Some("教育"));
    }

    #[test]
    fn unknown_combination_yields_no_badge() {
        let risk = Iprisk { company_type: "hosting".into(), ..Iprisk::default() };
        assert_eq!(risk.property_badge(), None);
        let empty = Iprisk::default();
        assert_eq!(empty.property_badge(), None);
    }
}
