//! 网络探测与 API 客户端层。
//!
//! 职责：net.coffee API 客户端、第三方探测源（cdn-cgi/trace、ip138、my.ip.cn、
//! CDN 响应头）、连通计时（TCP 连接 + TLS 握手）、DNS UDP 53 查询、手写 RFC 5389
//! STUN Binding。解析与判定均为纯函数（测试接缝），真实收发是薄 IO 壳。
//! 允许依赖：外部 crate 与标准库；不得依赖 `detect`/`ui`（依赖单向 ui → detect → net）。

pub mod cc;
pub mod cn_source;
pub mod geoip;
pub mod http;
pub mod iprisk;
pub mod latency;
pub mod split;
pub mod targets;
pub mod trace;

pub mod status;

pub mod ip_score;
