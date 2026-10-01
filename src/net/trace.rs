//! Cloudflare `/cdn-cgi/trace` 响应的获取与解析。
//!
//! trace 是各页共用的出口 IP 探测手法：向目标域名请求 `/cdn-cgi/trace`，
//! 得到 `key=value` 逐行文本，其中 `ip=` 是该站看到的出口 IP、`loc=` 是出口地区码。

/// 一次 trace 探测的解析结果；任一字段缺失即为 `None`（调用方视为探测失败）。
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Trace {
    pub ip: Option<String>,
    pub loc: Option<String>,
}

/// 解析 trace 文本：每行 `key=value`（首个 `=` 之后全部算值），
/// 取合法 IPv4/IPv6 的 `ip` 与 `loc`（地区码统一转小写）；其余行忽略。空行与坏行容忍。
pub fn parse_trace(text: &str) -> Trace {
    let mut trace = Trace::default();
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "ip" if trace.ip.is_none() && value.parse::<std::net::IpAddr>().is_ok() => {
                trace.ip = Some(value.to_string());
            }
            "loc" if trace.loc.is_none() => trace.loc = Some(value.to_lowercase()),
            _ => {}
        }
    }
    trace
}

/// 请求 `https://{host}/cdn-cgi/trace` 并解析；网络失败或解析不出 IP 时为 `None`。
pub async fn fetch_trace(client: &reqwest::Client, host: &str) -> Option<Trace> {
    let text = client
        .get(format!("https://{host}/cdn-cgi/trace"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let trace = parse_trace(&text);
    trace.ip.as_ref()?;
    Some(trace)
}

#[cfg(test)]
mod tests {
    use super::parse_trace;

    /// 报告 §3.8 的 1.1.1.1 trace 真实结构（字段节选）。
    const SAMPLE: &str = "fl=123abc\nh=y\nip=198.51.100.32\nts=1700000000.000\nvisit_scheme=https\nuag=Mozilla/5.0\ncolo=HKG\nsliver=none\nhttp=http/2\nloc=HK\ntls=TLSv1.3\nsni=plaintext\nwarp=off\n";

    #[test]
    fn invalid_trace_ip_is_a_failed_probe() {
        assert_eq!(parse_trace("ip=garbage\nloc=US\n").ip, None);
        assert_eq!(parse_trace("ip=999.1.2.3\n").ip, None);
        assert_eq!(
            parse_trace("ip=2606:4700:4700::1111\n").ip.as_deref(),
            Some("2606:4700:4700::1111")
        );
    }

    #[test]
    fn parses_ip_and_lowercased_loc_from_real_sample() {
        let trace = parse_trace(SAMPLE);
        assert_eq!(trace.ip.as_deref(), Some("198.51.100.32"));
        assert_eq!(trace.loc.as_deref(), Some("hk"));
    }

    #[test]
    fn empty_text_yields_nothing() {
        let trace = parse_trace("");
        assert_eq!(trace.ip, None);
        assert_eq!(trace.loc, None);
    }

    #[test]
    fn missing_ip_line_yields_no_ip() {
        let trace = parse_trace("fl=1\nloc=US\ntls=TLSv1.3\n");
        assert_eq!(trace.ip, None);
        assert_eq!(trace.loc.as_deref(), Some("us"));
    }

    #[test]
    fn crlf_line_endings_are_tolerated() {
        let trace = parse_trace("ip=1.2.3.4\r\nloc=JP\r\n");
        assert_eq!(trace.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(trace.loc.as_deref(), Some("jp"));
    }

    #[test]
    fn value_keeps_everything_after_first_equals() {
        let trace = parse_trace("ip=1.2.3.4\nweird=a=b=c\n");
        assert_eq!(trace.ip.as_deref(), Some("1.2.3.4"));
    }

    #[test]
    fn lines_without_equals_sign_are_ignored() {
        let trace = parse_trace("garbage line\nip=5.6.7.8\n=\nloc=CN\n");
        assert_eq!(trace.ip.as_deref(), Some("5.6.7.8"));
        assert_eq!(trace.loc.as_deref(), Some("cn"));
    }

    #[test]
    fn empty_value_is_treated_as_missing() {
        let trace = parse_trace("ip=\nloc=\n");
        assert_eq!(trace.ip, None);
        assert_eq!(trace.loc, None);
    }

    #[test]
    fn duplicate_keys_keep_first_value() {
        let trace = parse_trace("ip=1.1.1.1\nip=2.2.2.2\n");
        assert_eq!(trace.ip.as_deref(), Some("1.1.1.1"));
    }
}
