//! 国内双源出口探测：ip138 页面与 my.ip.cn 文本的解析。
//!
//! 双源并发请求，两源 IP 不同即「主出口 + 备用出口」分卡显示。
//! 解析规则依据接口报告：
//! - ip138：HTML 里首个 IPv4 + `来自：([^<\n]+)`；
//! - my.ip.cn：文本里首个 IPv4 + `归属地：(.+)`。

/// 一个国内源的探测结果。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CnSource {
    pub ip: String,
    pub location: String,
}

/// 从文本中提取首个合法 IPv4（各段 0-255）；找不到为 `None`。
pub fn first_ipv4(text: &str) -> Option<String> {
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            run.push(ch);
        } else if !run.is_empty() {
            if let Some(ip) = valid_ipv4(&run) {
                return Some(ip);
            }
            run.clear();
        }
    }
    valid_ipv4(&run)
}

/// 校验 `a.b.c.d` 形式且各段 ≤255；合法时原样返回。
fn valid_ipv4(run: &str) -> Option<String> {
    let octets: Vec<&str> = run.split('.').collect();
    if octets.len() != 4 {
        return None;
    }
    let ok = octets.iter().all(|o| {
        !o.is_empty()
            && o.len() <= 3
            && o.chars().all(|c| c.is_ascii_digit())
            && o.parse::<u16>().map(|v| v <= 255).unwrap_or(false)
    });
    ok.then(|| run.to_string())
}

/// 解析 ip138 页面：首个 IPv4 + `来自：`之后到 `<` 或行尾的归属地。
pub fn parse_ip138(html: &str) -> Option<CnSource> {
    let ip = first_ipv4(html)?;
    let location = after_label(html, "来自：", &['<', '\n']);
    Some(CnSource { ip, location })
}

/// 解析 my.ip.cn 文本：首个 IPv4 + `归属地：`之后的归属地。
pub fn parse_my_ip_cn(text: &str) -> Option<CnSource> {
    let ip = first_ipv4(text)?;
    let location = after_label(text, "归属地：", &['\n']);
    Some(CnSource { ip, location })
}

/// 取 `label` 之后到任一终止字符前的内容并去除首尾空白；未出现标签为空串。
fn after_label(text: &str, label: &str, terminators: &[char]) -> String {
    let Some((_, rest)) = text.split_once(label) else {
        return String::new();
    };
    let end = rest
        .find(|c: char| terminators.contains(&c))
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

/// 请求 ip138 页面并解析；失败为 `None`。
pub async fn fetch_ip138(client: &reqwest::Client) -> Option<CnSource> {
    let html = client
        .get("https://2026.ip138.com/")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    parse_ip138(&html)
}

/// 请求 my.ip.cn 文本并解析；失败为 `None`。
pub async fn fetch_my_ip_cn(client: &reqwest::Client) -> Option<CnSource> {
    let text = client
        .get("https://my.ip.cn/")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    parse_my_ip_cn(&text)
}

#[cfg(test)]
mod tests {
    use super::{first_ipv4, parse_ip138, parse_my_ip_cn};

    /// ip138 响应结构样例，地址与定位使用示例值（UTF-8）。
    const IP138_HTML: &str = "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\"/>\n<title>您的IP地址是：192.0.2.160</title>\n</head>\n<body>\n<div><span>来自：中国 示例省份 示例城市  示例运营商</span></div>\n</body>\n</html>\n";

    /// 报告 §3.1 的 my.ip.cn 响应格式。
    const MY_IP_CN_TEXT: &str = "ip：192.0.2.216 归属地：中国 示例省份 示例城市  示例运营商\n";

    #[test]
    fn parses_ip138_title_ip_and_from_line() {
        let source = parse_ip138(IP138_HTML).expect("样例应解析成功");
        assert_eq!(source.ip, "192.0.2.160");
        assert_eq!(source.location, "中国 示例省份 示例城市  示例运营商");
    }

    #[test]
    fn parses_my_ip_cn_plain_text() {
        let source = parse_my_ip_cn(MY_IP_CN_TEXT).expect("样例应解析成功");
        assert_eq!(source.ip, "192.0.2.216");
        assert_eq!(source.location, "中国 示例省份 示例城市  示例运营商");
    }

    #[test]
    fn location_is_trimmed() {
        let source = parse_my_ip_cn("ip：1.2.3.4 归属地：  日本 东京  \n").expect("应解析成功");
        assert_eq!(source.location, "日本 东京");
    }

    #[test]
    fn ipv4_octets_above_255_are_rejected() {
        assert_eq!(first_ipv4("ip：999.1.1.1 归属地：x"), None);
        assert_eq!(first_ipv4("ip：1.2.3.256 归属地：x"), None);
        assert_eq!(first_ipv4("没有 IP"), None);
    }

    #[test]
    fn first_ipv4_wins_over_later_ones() {
        assert_eq!(first_ipv4("a 10.0.0.1 b 10.0.0.2"), Some("10.0.0.1".into()));
    }

    #[test]
    fn ip138_without_from_line_still_yields_ip() {
        let source = parse_ip138("<title>您的IP地址是：8.8.8.8</title>").expect("应解析成功");
        assert_eq!(source.ip, "8.8.8.8");
        assert_eq!(source.location, "");
    }

    #[test]
    fn empty_and_error_pages_yield_none() {
        assert_eq!(parse_ip138(""), None);
        assert_eq!(parse_my_ip_cn(""), None);
        assert_eq!(parse_my_ip_cn("<title>502 Bad Gateway</title>"), None);
    }
}
