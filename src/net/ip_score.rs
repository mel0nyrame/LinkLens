//! 深度 IP 聚合与 v2 区块。解析为纯函数，HTTP 收发为薄 IO 壳。
use super::iprisk::Iprisk;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Lookup {
    #[serde(flatten)]
    pub risk: Iprisk,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub registered_country_code: String,
    #[serde(default)]
    pub registered_country: String,
    #[serde(default)]
    pub is_public_service: bool,
    #[serde(default)]
    pub is_bogon: bool,
    #[serde(default)]
    pub bogon_reason: String,
    #[serde(default)]
    pub bogon_rfc: String,
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub is_mobile: bool,
    #[serde(default)]
    pub asn_kind: String,
    #[serde(default)]
    pub asn_tbps: String,
    #[serde(default)]
    pub asn_ipv4_count: Option<u64>,
    #[serde(default)]
    pub asn_allocated: String,
    #[serde(default)]
    pub asname: String,
    #[serde(default)]
    pub rdns: String,
    #[serde(default)]
    pub isp: String,
    #[serde(default)]
    pub rpki_status: String,
    #[serde(default)]
    pub datacenter_name: String,
    #[serde(default)]
    pub abuser_score: Value,
    #[serde(default)]
    pub reddit_blocked: Option<bool>,
    #[serde(default)]
    pub range: Value,
    #[serde(default)]
    pub public_service: Value,
    #[serde(default)]
    pub vpn_trace: Value,
    #[serde(default)]
    pub intelligence: Value,
    #[serde(default)]
    pub ai_verdict: Option<AiVerdict>,
    #[serde(default)]
    pub geo_sources: Vec<GeoSource>,
    #[serde(default)]
    pub location_history: Vec<Value>,
    #[serde(default)]
    pub asn_history: Vec<Value>,
    #[serde(default)]
    pub company_history: Vec<Value>,
    #[serde(default)]
    pub dc_neighbors: Vec<Value>,
    #[serde(default)]
    pub related_domains: Vec<Domain>,
    #[serde(default)]
    pub related_domains_pending: bool,
}

pub fn parse_lookup(text: &str) -> Result<Lookup, serde_json::Error> {
    serde_json::from_str(text)
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AiVerdict {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub reasoning: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GeoSource {
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub country_code: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    #[serde(default)]
    pub accuracy_km: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Domain {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub via: String,
}

impl Lookup {
    /// 完整坐标优先级 g1 > g7 > g3 > g2，缺任一坐标则跳过该源。
    pub fn best_coordinates(&self) -> Option<(f64, f64)> {
        ["g1", "g7", "g3", "g2"].into_iter().find_map(|src| {
            self.geo_sources
                .iter()
                .filter(|g| g.src == src)
                .find_map(|g| g.lat.zip(g.lon))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError(pub String, Option<u16>);

impl ApiError {
    pub fn new(message: String) -> Self {
        Self(message, None)
    }
    pub fn status(&self) -> Option<u16> {
        self.1
    }
    /// 深度接口400是终态，上游接口报告不重试也不请求v2兜底。
    pub fn retryable(&self) -> bool {
        self.1 != Some(400)
    }
}

pub fn decode_response<T: serde::de::DeserializeOwned>(
    endpoint: &str,
    status: u16,
    body: &str,
) -> Result<T, ApiError> {
    if !(200..300).contains(&status) {
        let detail = match status {
            400 => "IP 无效或查询请求被拒绝",
            502 => "上游归属地服务暂不可用",
            429 => "请求过于频繁，请稍后重试",
            _ => "服务暂不可用",
        };
        return Err(ApiError(
            format!("{endpoint} HTTP {status}：{detail}"),
            Some(status),
        ));
    }
    serde_json::from_str(body).map_err(|_| ApiError::new(format!("{endpoint}响应格式不合法")))
}

/// 独立 API 时限（上游接口报告深度45s、增强14-22s），覆盖共用客户端的8s公网探测时限。
pub async fn fetch<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    path: &str,
    endpoint: &str,
    timeout: std::time::Duration,
) -> Result<T, ApiError> {
    let response = client
        .get(format!("{}{path}", super::geoip::API_BASE))
        .timeout(timeout)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| {
            ApiError::new(format!(
                "{endpoint}{}",
                if e.is_timeout() {
                    "超时"
                } else {
                    "不可达"
                }
            ))
        })?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|_| ApiError::new(format!("{endpoint}响应读取失败")))?;
    decode_response(endpoint, status, &body)
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Related {
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub related_domains: Vec<Domain>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Heat {
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub idx: Option<u32>,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub vals: Vec<u64>,
    #[serde(default)]
    pub days: Vec<String>,
    #[serde(default)]
    pub chg: Option<f64>,
    #[serde(default)]
    pub peak: Value,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Bgp {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub paths: u64,
    #[serde(default)]
    pub origins: Vec<BgpNode>,
    #[serde(default)]
    pub upstreams: Vec<BgpNode>,
    #[serde(default)]
    pub second: Vec<BgpNode>,
    #[serde(default)]
    pub snapshots: Vec<Value>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BgpNode {
    #[serde(default)]
    pub asn: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub share: f64,
    #[serde(default)]
    pub tier1: bool,
    #[serde(default)]
    pub via: Option<u32>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AsnCompanies {
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub companies: Vec<AsnCompany>,
    #[serde(default)]
    pub total_profiles: u64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AsnCompany {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub n: u64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Radar {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub human: Option<f64>,
    #[serde(default)]
    pub bot: Option<f64>,
    #[serde(default)]
    pub range: String,
    #[serde(default)]
    pub cached: bool,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Dnsbl {
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub results: Vec<DnsblResult>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DnsblResult {
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub zone: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub listed: bool,
    #[serde(default)]
    pub codes: Vec<String>,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    pub status: String,
}
impl DnsblResult {
    pub fn verdict(&self) -> &'static str {
        if self.status == "timeout" {
            return "超时";
        }
        if !matches!(self.status.as_str(), "ok" | "nxdomain") {
            return "查询失败";
        }
        if self.listed
            && self.zone.to_ascii_lowercase().contains("spamhaus")
            && self
                .codes
                .iter()
                .all(|c| matches!(c.as_str(), "127.0.0.10" | "127.0.0.11"))
        {
            "纯净（PBL 网段策略）"
        } else if self.listed {
            "黑名单"
        } else {
            "纯净"
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollDecision {
    Done,
    Again,
    TimedOut,
}

pub fn poll_decision(pending: bool, attempt: u8, limit: u8) -> PollDecision {
    if !pending {
        PollDecision::Done
    } else if attempt >= limit {
        PollDecision::TimedOut
    } else {
        PollDecision::Again
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn google_lookup_fixture_preserves_deep_fields_and_source_priority() {
        let d = parse_lookup(include_str!("../../tests/fixtures/ip-lookup-google.json")).unwrap();
        assert_eq!(d.risk.ip, "8.8.8.8");
        assert_eq!(d.risk.trust_score, Some(63));
        assert_eq!(d.ai_verdict.as_ref().unwrap().label, "公共 DNS 任播");
        assert_eq!(d.ai_verdict.as_ref().unwrap().confidence, Some(100.0));
        assert_eq!(d.best_coordinates(), Some((39.03, -77.5)));
        assert_eq!(d.geo_sources.len(), 4);
        assert_eq!(d.registered_country_code, "us");
        assert!(d.is_public_service);
        assert!(d.location_history.is_empty());
        assert_eq!(d.related_domains[0].domain, "dns.google");
    }
    #[test]
    fn lookup_bad_input_and_geo_gateway_failure_have_distinct_messages() {
        let lookup =
            decode_response::<Lookup>("深度查询", 400, "<html>bad request</html>").unwrap_err();
        let geo = decode_response::<Lookup>("归属地", 502, "<html>bad gateway</html>").unwrap_err();
        assert_eq!(lookup.status(), Some(400));
        assert!(!lookup.retryable());
        assert!(geo.retryable());
        assert!(lookup.0.contains("400"));
        assert!(lookup.0.contains("IP"));
        assert!(geo.0.contains("502"));
        assert!(geo.0.contains("上游"));
        assert_ne!(lookup, geo);
        assert!(
            decode_response::<Lookup>("深度查询", 200, "not json")
                .unwrap_err()
                .0
                .contains("格式")
        );
    }

    #[test]
    fn bgp_and_enhancement_samples_keep_route_shares_and_pending() {
        let bgp: Bgp = decode_response(
            "BGP",
            200,
            include_str!("../../tests/fixtures/ip-bgp-google.json"),
        )
        .unwrap();
        assert_eq!(bgp.paths, 374);
        assert_eq!(bgp.origins[0].share, 100.0);
        assert_eq!(bgp.upstreams[0].asn, 214292);
        assert_eq!(bgp.second[0].via, Some(214292));
        let heat: Heat = decode_response("热度", 200, r#"{"supported":true,"base":"45.12.33","idx":6,"mode":"冷门","vals":[13,15],"days":["2026-09-16","2026-09-17"],"chg":3,"peak":{"i":1,"v":15,"day":"2026-09-17"}}"#).unwrap();
        assert_eq!(heat.vals, vec![13, 15]);
        assert_eq!(heat.peak["v"], 15);
        let related: Related = decode_response(
            "反查",
            200,
            r#"{"pending":true,"related_domains":[{"domain":"dns.google","via":"reverse DNS"}]}"#,
        )
        .unwrap();
        assert!(related.pending);
        let companies: AsnCompanies = decode_response("同ASN", 200, r#"{"asn":15169,"companies":[{"name":"Google LLC","type":"hosting","n":1467}],"total_profiles":1897}"#).unwrap();
        assert_eq!(companies.companies[0].n, 1467);
        assert_eq!(companies.total_profiles, 1897);
        let radar: Radar = decode_response("Radar", 200, r#"{"asn":15169,"ok":true,"human":2.66,"bot":97.34,"range":"28d","cached":true,"stale":false}"#).unwrap();
        assert_eq!(radar.human, Some(2.66));
    }

    #[test]
    fn dnsbl_spamhaus_policy_list_is_clean_but_network_errors_are_unknown() {
        let d: Dnsbl = decode_response("DNSBL", 200, r#"{"supported":true,"results":[{"engine":"Spamhaus ZEN","zone":"zen.spamhaus.org","listed":true,"codes":["127.0.0.10","127.0.0.11"],"status":"ok"},{"engine":"Spamhaus ZEN","zone":"zen.spamhaus.org","listed":true,"codes":["127.0.0.10","127.0.0.2"],"status":"ok"},{"listed":false,"status":"timeout"},{"listed":false,"status":"rcode2"},{"listed":false,"status":"nxdomain"}]}"#).unwrap();
        assert!(d.supported);
        assert_eq!(d.results[0].verdict(), "纯净（PBL 网段策略）");
        assert_eq!(d.results[1].verdict(), "黑名单");
        assert_eq!(d.results[2].verdict(), "超时");
        assert_eq!(d.results[3].verdict(), "查询失败");
        assert_eq!(d.results[4].verdict(), "纯净");
    }
    #[test]
    fn pending_polling_finishes_or_stops_at_the_ticket_budget() {
        assert_eq!(poll_decision(false, 10, 10), PollDecision::Done);
        assert_eq!(poll_decision(true, 9, 10), PollDecision::Again);
        assert_eq!(poll_decision(true, 10, 10), PollDecision::TimedOut);
        assert_eq!(poll_decision(true, 11, 12), PollDecision::Again);
        assert_eq!(poll_decision(true, 12, 12), PollDecision::TimedOut);
    }
    #[test]
    fn bogon_fixture_keeps_reason_and_absent_asn_without_inventing_geo() {
        let d = parse_lookup(include_str!("../../tests/fixtures/ip-lookup-bogon.json")).unwrap();
        assert!(d.is_bogon);
        assert_eq!(d.bogon_reason, "Loopback");
        assert_eq!(d.bogon_rfc, "RFC1122");
        assert_eq!(d.risk.asn, None);
        assert_eq!(d.best_coordinates(), None);
        assert_eq!(d.src, "g0");
    }
}
