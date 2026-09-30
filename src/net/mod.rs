//! 网络探测与 API 客户端层（后续票填充）。
//!
//! 职责：net.coffee API 客户端、第三方探测源（cdn-cgi/trace、ip138、my.ip.cn、
//! CDN 响应头）、DNS UDP 53 查询、手写 RFC 5389 STUN Binding。
//! 允许依赖：外部 crate 与标准库；不得依赖 `detect`/`ui`（依赖单向 ui → detect → net）。
