//! 检测器纯逻辑层。
//!
//! 职责：判定与评分（场景评分、受限地区表、DNS/WebRTC 泄漏判定、信任分档位、
//! 历史去重等），全部为纯函数，是单元测试的主战场。
//! 允许依赖：`net` 与外部 crate；不得依赖 `ui`（依赖单向 ui → detect → net）。

pub mod ai;
pub mod scene;
pub mod leak;
