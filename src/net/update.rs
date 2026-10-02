//! GitHub Release 的有界 HTTP IO；判定在 detect 层。
use serde::Deserialize;
use std::time::Duration;

pub const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub async fn fetch(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "LinkLens")
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("更新请求失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("更新服务返回错误：{e}"))?;
    if url.starts_with("https://") && response.url().scheme() != "https" {
        return Err("更新服务不允许降级到明文 HTTP".into());
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("更新响应超过大小上限".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("读取更新响应失败：{e}"))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("更新响应超过大小上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn response_server(response: Vec<u8>, delay: Duration) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0; 4096]);
            std::thread::sleep(delay);
            let _ = stream.write_all(&response);
        });
        format!("http://{address}/fixture")
    }

    #[tokio::test]
    async fn declared_archive_and_streamed_metadata_limits_are_enforced() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = response_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 134217729\r\nConnection: close\r\n\r\n".to_vec(),
            Duration::ZERO,
        );
        assert!(
            fetch(&client, &url, DOWNLOAD_TIMEOUT, 128 * 1024 * 1024)
                .await
                .unwrap_err()
                .contains("大小上限")
        );
        let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        response.extend(vec![b'x'; MAX_METADATA_BYTES + 1]);
        let url = response_server(response, Duration::ZERO);
        assert!(
            fetch(&client, &url, CHECK_TIMEOUT, MAX_METADATA_BYTES)
                .await
                .unwrap_err()
                .contains("大小上限")
        );
    }

    #[tokio::test]
    async fn version_check_times_out_before_slow_response() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = response_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
            Duration::from_secs(6),
        );
        let started = std::time::Instant::now();
        assert!(
            fetch(&client, &url, CHECK_TIMEOUT, MAX_METADATA_BYTES)
                .await
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(5800));
    }
}
