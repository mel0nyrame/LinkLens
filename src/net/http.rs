//! 共享 HTTP 客户端：公网探测与 API 请求共用的 reqwest 配置。
//!
//! 质量红线：公网探测默认总超时 8 秒（覆盖连接到响应体读取）；聚合 API 按端点覆盖请求时限。

use std::time::Duration;

use reqwest::Client;

/// 公网探测统一超时（秒）。探测源使用此上限，聚合 API 可覆盖请求时限。
pub const PUBLIC_TIMEOUT: Duration = Duration::from_secs(8);

/// 构建全项目共用的探测客户端：rustls-TLS、8 秒总超时、常规浏览器 UA。
///
/// 代理遵循 `http_proxy`/`https_proxy` 环境变量（reqwest 默认行为），
/// 这正是本工具要测的「当前出口」。
pub fn client() -> Client {
    Client::builder()
        .timeout(PUBLIC_TIMEOUT)
        .user_agent("Mozilla/5.0")
        .build()
        .expect("探测客户端配置是静态合法的，构建不应失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_timeout_is_eight_seconds() {
        assert_eq!(PUBLIC_TIMEOUT, Duration::from_secs(8));
        // 客户端能以红线配置构建（rustls-TLS、超时、UA 均为静态合法配置）
        let _ = client();
    }
}
