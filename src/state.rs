//! 共享应用状态：后台探测任务写入、渲染线程读取。
//!
//! 探测任务（`probe`）在完成每一步时更新本状态并唤醒事件循环重绘；
//! 渲染侧只做快照读取。这里只放数据形状，不放探测编排与渲染。

use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Notify;

use crate::app::App;
use crate::net::latency::{LatencyTier, RoundResult, tier};
use crate::net::split::SplitExit;
use crate::state_ai::AiState;
use crate::state_leak::{DnsLeakState, WebrtcState};

/// 首页一张出口 IP 卡的内容。
#[derive(Clone, Debug, Default)]
pub struct EgressCard {
    /// 卡片标题：主出口 / 备用出口 / Cloudflare 出口。
    pub label: String,
    pub ip: String,
    /// 中文归属地（可能来自 GeoIP 纠正重拼）。
    pub geo: String,
    /// 来源标注：iP138.com / IP.cn / GeoIP（地区库纠正）等。
    pub source: String,
    /// 国旗 emoji。
    pub flag: String,
    /// IP 属性徽章（家庭宽带 / 机房IP / 商业专线 / 教育）。
    pub badges: Vec<&'static str>,
}

/// 出口探测阶段。
#[derive(Clone, Debug, Default)]
pub enum EgressPhase {
    /// 探测中。
    #[default]
    Pending,
    /// 双源出口与 Cloudflare 出口就绪。
    Ready(Vec<EgressCard>),
}

/// 一个连通测量目标的进度（首页 6 目标小卡与 47 目标页共用）。
#[derive(Clone, Debug)]
pub struct LatencyState {
    pub name: &'static str,
    /// 已完成的测量轮（不含预热）。
    pub rounds: Vec<RoundResult>,
    /// 至今成功轮的中位数。
    pub median: RoundResult,
    /// 全部轮次是否完成。
    pub done: bool,
}

impl LatencyState {
    /// 以目标名初始化（零轮、未完成）。
    pub fn new(name: &'static str) -> LatencyState {
        LatencyState {
            name,
            rounds: Vec::new(),
            median: None,
            done: false,
        }
    }

    /// 追加一轮结果并重算中位数（成功轮参与，失败轮只记档）。
    pub fn push_round(&mut self, result: RoundResult) {
        self.rounds.push(result);
        let successes: Vec<u64> = self.rounds.iter().flatten().copied().collect();
        self.median = crate::net::latency::median(&successes);
    }

    /// 当前分档（用于取色与「超时」文案）。
    pub fn tier(&self) -> LatencyTier {
        tier(self.median)
    }

    /// 已达成的成功轮数。
    pub fn success_count(&self) -> usize {
        self.rounds.iter().flatten().count()
    }
}

/// 分流探测的单站状态（下标与 `net::split::SITES` 对齐）。
#[derive(Clone, Debug, Default)]
pub struct SplitSiteState {
    /// 探测到的出口 IP；失败为 `None`。
    pub ip: Option<String>,
}

/// 首页状态。
#[derive(Clone, Debug, Default)]
pub struct HomeState {
    pub egress: EgressPhase,
    /// 首页 6 目标（顺序与 `probe::HOME_LATENCY_TARGETS` 对齐）。
    pub latency: Vec<LatencyState>,
    /// 分流单站结果（顺序与 `net::split::SITES` 对齐）。
    pub split: Vec<SplitSiteState>,
    /// 分流出口去重汇总（全部站点探测完成后生成）。
    pub split_summary: Vec<SplitExit>,
    /// 分流探测是否全部结束。
    pub split_done: bool,
}

impl HomeState {
    /// 已拿到出口 IP 的分流站数。
    pub fn split_resolved(&self) -> usize {
        self.split.iter().filter(|s| s.ip.is_some()).count()
    }
}

/// 全部目标的连通测量状态。
#[derive(Clone, Debug, Default)]
pub struct LinkState {
    /// 首次进入页面时启动；重测保留此标记。
    pub started: bool,
    /// 新一轮递增，迟到的旧轮次结果不得写入当前测量。
    pub generation: u64,
    /// 目标进度（顺序与 `net::targets::TARGETS` 对齐）。
    pub targets: Vec<LatencyState>,
}

impl LinkState {
    pub fn begin_probe(&mut self, refresh: bool) -> Option<u64> {
        if self.started && !refresh {
            return None;
        }
        self.started = true;
        self.generation += 1;
        self.targets = crate::net::targets::TARGETS
            .iter()
            .map(|t| LatencyState::new(t.name))
            .collect();
        Some(self.generation)
    }

    pub fn push_round(&mut self, generation: u64, index: usize, result: RoundResult) -> bool {
        if generation != self.generation {
            return false;
        }
        self.targets[index].push_round(result);
        true
    }

    pub fn finish_target(&mut self, generation: u64, index: usize) -> bool {
        if generation != self.generation {
            return false;
        }
        self.targets[index].done = true;
        true
    }
}

#[cfg(test)]
mod link_tests {
    use super::LinkState;

    #[test]
    fn refresh_clears_rounds_and_rejects_late_results() {
        let mut state = LinkState::default();
        let first = state.begin_probe(false).unwrap();
        state.push_round(first, 0, Some(42));
        state.finish_target(first, 0);
        assert_eq!(state.begin_probe(false), None);
        let second = state.begin_probe(true).unwrap();
        assert!(
            state
                .targets
                .iter()
                .all(|t| t.rounds.is_empty() && t.median.is_none() && !t.done)
        );
        assert!(!state.push_round(first, 0, Some(99)));
        assert!(!state.finish_target(first, 0));
        assert!(state.push_round(second, 0, Some(7)));
        assert_eq!(state.targets[0].median, Some(7));
        let third = state.begin_probe(true).unwrap();
        assert!(!state.push_round(second, 0, Some(42)));
        assert!(state.finish_target(third, 0));
    }
}

/// 整个应用的状态：键位状态 + 首页 + 连通页 + AI 检测页 + 泄漏检测两页。
#[derive(Default)]
pub struct AppState {
    pub app: App,
    pub home: HomeState,
    pub link: LinkState,
    /// Claude/GPT 检测页状态（详见 `state_ai`）。
    pub ai: AiState,
    pub score: crate::state_score::ScoreState,
    /// DNS 泄漏页（形状见 `state_leak`）。
    pub dns_leak: DnsLeakState,
    /// WebRTC 泄漏页（形状见 `state_leak`）。
    pub webrtc: WebrtcState,
}

/// 线程安全的共享句柄：互斥锁 + 变更通知。
#[derive(Clone)]
pub struct SharedState {
    state: Arc<Mutex<AppState>>,
    notify: Arc<Notify>,
}

impl SharedState {
    /// 初始化为默认状态。
    pub fn new() -> SharedState {
        SharedState {
            state: Arc::new(Mutex::new(AppState::default())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 短暂加锁读取/修改状态（调用方不得在持有锁期间 await）。
    pub fn lock(&self) -> MutexGuard<'_, AppState> {
        self.state.lock().expect("状态锁不应中毒")
    }

    /// 通知事件循环有数据更新、应当重绘。
    pub fn notify(&self) {
        self.notify.notify_one();
    }

    /// 等待下一次数据更新（事件循环在 select 中挂起于此）。
    pub async fn changed(&self) {
        self.notify.notified().await;
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}
