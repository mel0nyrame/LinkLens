//! 泄漏检测两页（DNS / WebRTC）的共享状态：后台探测任务写入、渲染线程读取。
//!
//! 数据形状沿用 `state.rs` 的模式；判定结论直接复用 `detect::leak` 的枚举，
//! 这里只放展示态与进度计数，不放探测编排与判定规则。

use crate::detect::leak::{WebrtcCandidate, WebrtcVerdict};
use crate::net::dnsleak::{DEEP_ROUNDS, FAST_ROUNDS};

/// DNS 探测模式（上游接口报告「快速测试 / 深度测试」）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DnsMode {
    /// 快速测试：5 轮。
    Fast,
    /// 深度测试：8 轮。
    Deep,
}

impl DnsMode {
    /// 本模式的查询轮数（报告 §3.6）。
    pub fn rounds(self) -> u32 {
        match self {
            DnsMode::Fast => FAST_ROUNDS,
            DnsMode::Deep => DEEP_ROUNDS,
        }
    }

    /// 展示文案（含轮数，与 `rounds` 保持一致）。
    pub fn label(self) -> String {
        match self {
            DnsMode::Fast => format!("快速测试（{} 轮）", self.rounds()),
            DnsMode::Deep => format!("深度测试（{} 轮）", self.rounds()),
        }
    }
}

/// DNS 泄漏页探测阶段。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DnsPhase {
    /// 等待用户按键发起（f 快速 / d 深度）。
    #[default]
    Idle,
    /// 探测进行中。
    Running,
    /// 探测结束（有结论或失败）。
    Done,
}

/// 一条 DNS 解析器出口记录（带国旗与中文归属地）。
#[derive(Clone, Debug, Default)]
pub struct ResolverEntry {
    pub ip: String,
    /// 国旗 emoji。
    pub flag: String,
    /// 中文归属地。
    pub location: String,
}

/// DNS 泄漏页状态。
#[derive(Clone, Debug, Default)]
pub struct DnsLeakState {
    pub phase: DnsPhase,
    /// 本次探测模式；未发起为 `None`。
    pub mode: Option<DnsMode>,
    /// 已完成的触发轮数。
    pub rounds_done: u32,
    /// 计划的总轮数。
    pub rounds_total: u32,
    /// HTTP 出口参照（1.1.1.1 trace）。
    pub egress_ip: Option<String>,
    pub egress_flag: String,
    pub egress_geo: String,
    /// 出口国别码（小写），供判定使用。
    pub egress_country: String,
    /// 解析器出口列表（顺序与回读接口一致）。
    pub resolvers: Vec<ResolverEntry>,
    /// 三态判定结论。
    pub verdict: Option<crate::detect::leak::DnsVerdict>,
    /// 探测失败原因（区别于「已加密未暴露」的空列表）。
    pub error: Option<String>,
}

/// WebRTC 页探测阶段。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WebrtcPhase {
    /// 等待探测（进入页面自动开始一次）。
    #[default]
    Idle,
    /// STUN 采集进行中。
    Running,
    /// 采集结束（有结论或失败）。
    Done,
}

/// WebRTC 页状态。
#[derive(Clone, Debug, Default)]
pub struct WebrtcState {
    pub phase: WebrtcPhase,
    /// HTTP 出口参照（1.1.1.1 trace）。
    pub egress_ip: Option<String>,
    pub egress_flag: String,
    pub egress_geo: String,
    /// 过滤私网后的候选清单（公网 STUN 地址）。
    pub candidates: Vec<WebrtcCandidate>,
    /// 已完成的 STUN 查询数。
    pub queries_done: usize,
    /// 计划的 STUN 查询总数（3 服务器 × 最多双栈）。
    pub queries_total: usize,
    /// 泄漏判定结论。
    pub verdict: Option<WebrtcVerdict>,
    /// 探测失败原因（区别于「未暴露」的零候选）。
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::DnsMode;
    use crate::net::dnsleak::{DEEP_ROUNDS, FAST_ROUNDS};

    #[test]
    fn dns_mode_rounds_and_labels_agree() {
        assert_eq!(DnsMode::Fast.rounds(), FAST_ROUNDS);
        assert_eq!(DnsMode::Deep.rounds(), DEEP_ROUNDS);
        assert_eq!(DnsMode::Fast.label(), "快速测试（5 轮）");
        assert_eq!(DnsMode::Deep.label(), "深度测试（8 轮）");
        // 标签里的轮数与 rounds() 一致，避免改常量后文案漂移
        for mode in [DnsMode::Fast, DnsMode::Deep] {
            assert!(mode.label().contains(&mode.rounds().to_string()));
        }
    }
}
