//! AI 检测页（Claude/GPT）状态：每页一份，含探测进度、完整结果与检测历史。
//!
//! 这里只放数据形状（渲染快照读、探测任务写），判定逻辑在 `detect::ai`、
//! 编排在 `probe_ai`。

use crate::app::Page;
use crate::history::HistoryEntry;
use crate::net::geoip::GeoIp;
use crate::net::iprisk::Iprisk;
use crate::net::status::ServiceStatus;
use crate::net::trace::Trace;
use crate::state::EgressCard;

/// 一次 AI 出口检测的完整结果。
#[derive(Clone, Debug)]
pub struct AiOutcome {
    /// 本轮并行采集的国内与 Cloudflare 出口，独立于首页启动快照。
    pub reference_egress: Vec<EgressCard>,
    /// AI 出口 trace（`ip=` 出口 IP 与 `loc=` 地区码）。
    pub exit: Option<Trace>,
    /// 出口 IP 的风险库记录（响应 `ip` 可能是 /24 段代表 IP）。
    pub risk: Option<Iprisk>,
    /// 出口 IP 的 geoip 归属地记录。
    pub geo: Option<GeoIp>,
    /// geoip 拿不到国别码、由 trace `loc=` 兜底（IPv6 出口常见）。
    pub geo_from_trace: bool,
    /// 出口命中受限地区（信任分强制 0、不显示时延）。
    pub restricted: bool,
    /// 可用性探测逐目标时延（顺序与 profile 的 availability_targets 对齐）。
    pub availability: Vec<(&'static str, Option<u64>)>,
    /// 服务状态（status.json）。
    pub status: Option<ServiceStatus>,
}

/// AI 页探测阶段。
#[derive(Clone, Debug, Default)]
pub enum AiPhase {
    /// 探测中。
    #[default]
    Pending,
    /// 检测完成（探测失败也在 Done 里以空结果呈现）。
    Done(Box<AiOutcome>),
}

/// 一个 AI 检测页的状态。
#[derive(Clone, Debug, Default)]
pub struct AiPageState {
    /// 探测是否已启动（进入页面时触发一次，幂等防重入）。
    pub started: bool,
    pub phase: AiPhase,
    /// 本页检测历史（最新在前；跨重启从 `.data/` 读回）。
    pub history: Vec<HistoryEntry>,
}

impl AiPageState {
    /// 认领一轮探测；调用方持锁时同时更新启动标记与阶段。
    pub fn begin_probe(&mut self) -> bool {
        if self.probing() {
            return false;
        }
        self.started = true;
        self.phase = AiPhase::Pending;
        true
    }

    /// 是否正在探测中。
    pub fn probing(&self) -> bool {
        self.started && matches!(self.phase, AiPhase::Pending)
    }
}

/// 两个 AI 检测页的状态（键为 `Page::Claude` / `Page::Gpt`）。
#[derive(Clone, Debug, Default)]
pub struct AiState {
    pub claude: AiPageState,
    pub gpt: AiPageState,
}

impl AiState {
    /// 按页取状态；非 AI 页返回首页侧不可达的 `None`。
    pub fn page(&self, page: Page) -> Option<&AiPageState> {
        match page {
            Page::Claude => Some(&self.claude),
            Page::Gpt => Some(&self.gpt),
            _ => None,
        }
    }

    /// 按页取可变状态；非 AI 页返回 `None`。
    pub fn page_mut(&mut self, page: Page) -> Option<&mut AiPageState> {
        match page {
            Page::Claude => Some(&mut self.claude),
            Page::Gpt => Some(&mut self.gpt),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_probe_claims_pending_atomically_and_rejects_duplicate_starts() {
        let mut page = AiPageState::default();
        assert!(page.begin_probe());
        assert!(page.started && page.probing());
        assert!(!page.begin_probe());
        page.phase = AiPhase::Done(Box::new(AiOutcome {
            reference_egress: Vec::new(),
            exit: None,
            risk: None,
            geo: None,
            geo_from_trace: false,
            restricted: false,
            availability: Vec::new(),
            status: None,
        }));
        assert!(page.begin_probe());
        assert!(page.started && page.probing());
        assert!(!page.begin_probe());
    }
}
