//! net.coffee 服务状态 JSON 的获取与解析。
//!
//! 权威依据：net.coffee 接口报告 §2.10 —— `/claude/status.json` 与 `/gpt/status.json`
//!（gpt 页带 `?t=时间戳` 防缓存）。schema：`overall`（中文结论文本）、
//! `overall_indicator ∈ none|minor|major|critical|maintenance`、`components[]`。

use serde::Deserialize;

use super::geoip::API_BASE;

/// 一个服务组件的状态行（如 claude.ai / ChatGPT 对话）。
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
pub struct StatusComponent {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    /// 官方转写的中文状态（如「正常运行」）。
    #[serde(rename = "status_cn", default)]
    pub status_cn: String,
}

/// 一次服务状态的解析结果。
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
pub struct ServiceStatus {
    /// 中文结论文本（如「全部正常」），界面直接展示。
    #[serde(default)]
    pub overall: String,
    /// 级别码 none|minor|major|critical|maintenance（判定走 detect::ai::status_level）。
    #[serde(rename = "overall_indicator", default)]
    pub overall_indicator: String,
    #[serde(default)]
    pub components: Vec<StatusComponent>,
}

/// 解析 status.json 文本；结构不合法为 `None`。
pub fn parse_status(json: &str) -> Option<ServiceStatus> {
    serde_json::from_str(json).ok()
}

/// 拉取服务状态；`path` 为 `claude/status.json` 或 `gpt/status.json`，带防缓存时间戳。
/// 网络失败或解析失败为 `None`。
pub async fn fetch_status(client: &reqwest::Client, path: &str) -> Option<ServiceStatus> {
    let bust = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let url = format!("{API_BASE}/{path}");
    client
        .get(url)
        .query(&[("t", bust.to_string())])
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
        .as_deref()
        .and_then(parse_status)
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    /// 报告 §2.10 的 /claude/status.json 真实样例（节选）。
    const SAMPLE_JSON: &str = r#"{"overall": "全部正常", "overall_indicator": "none",
 "components": [{"name": "claude.ai", "status": "operational", "status_cn": "正常运行"},
                {"name": "Claude Console", "status": "operational", "status_cn": "正常运行"}],
 "incidents": [{"name": "...", "name_cn": "...", "status": "resolved", "impact": "major",
                "created_at": "2026-09-29T14:21:37.188Z", "updated_at": "..."}],
 "fetched_at": "2026-09-30T16:49:32+00:00"}"#;

    #[test]
    fn sample_deserializes_overall_and_components() {
        let status = parse_status(SAMPLE_JSON).expect("样例应可解析");
        assert_eq!(status.overall, "全部正常");
        assert_eq!(status.overall_indicator, "none");
        assert_eq!(status.components.len(), 2);
        assert_eq!(status.components[0].name, "claude.ai");
        assert_eq!(status.components[0].status_cn, "正常运行");
    }

    #[test]
    fn incidents_and_fetched_at_are_tolerated() {
        // incidents/fetched_at 不进结构体也不报错
        let status = parse_status(SAMPLE_JSON).expect("样例应可解析");
        assert_eq!(status.components[1].name, "Claude Console");
    }

    #[test]
    fn degraded_status_parses_indicator() {
        let json = r#"{"overall": "轻微故障", "overall_indicator": "minor", "components": []}"#;
        let status = parse_status(json).expect("应可解析");
        assert_eq!(status.overall_indicator, "minor");
        assert!(status.components.is_empty());
    }

    #[test]
    fn garbage_and_empty_yield_none() {
        assert_eq!(parse_status(""), None);
        assert_eq!(parse_status("error code: 502"), None);
    }

    /// 集成冒烟（需真实网络，默认跳过）：`cargo test --lib net::status -- --ignored`
    #[tokio::test]
    #[ignore = "需要真实公网，手动执行"]
    async fn live_status_smoke() {
        let client = crate::net::http::client();
        let status = super::fetch_status(&client, "claude/status.json")
            .await
            .expect("claude status.json 应可拉取");
        assert!(!status.overall.is_empty());
    }
}
