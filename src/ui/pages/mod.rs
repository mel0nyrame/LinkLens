//! 七个功能页及可滚动画布入口；评分页保留固定输入栏。

pub mod ai;
pub mod claude;
pub mod connectivity;
pub mod dns_leak;
pub mod gpt;
pub mod ip_query;
pub mod ip_score;
pub mod webrtc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::app::Page;
use crate::state::AppState;

use crate::net::latency::{LatencyTier, tier};
use crate::state::LatencyState;
use crate::theme::color::{
    THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_SUCCESS_SOFT, THEME_WARNING,
};

pub fn canvas(width: u16, state: &AppState) -> Buffer {
    match state.app.page {
        Page::IpQuery => ip_query::canvas(width, &state.home, state.app.hide_ip),
        Page::Claude => claude::canvas(width, state),
        Page::Gpt => gpt::canvas(width, state),
        Page::DnsLeak => dns_leak::canvas(width, &state.dns_leak, state.app.hide_ip),
        Page::WebRtc => webrtc::canvas(width, &state.webrtc, state.app.hide_ip),
        Page::Connectivity => connectivity::canvas(width, &state.link),
        Page::IpScore => Buffer::empty(Rect::default()),
    }
}

pub fn max_scroll(area: Rect, state: &AppState) -> u16 {
    canvas(area.width, state)
        .area
        .height
        .saturating_sub(area.height)
}

/// 分档色：<100ms 绿 / <400ms 浅绿 / 其余黄 / 失败红。
pub(crate) fn latency_color(tier_value: LatencyTier) -> Color {
    match tier_value {
        LatencyTier::Fast => THEME_SUCCESS,
        LatencyTier::Good => THEME_SUCCESS_SOFT,
        LatencyTier::Slow => THEME_WARNING,
        LatencyTier::Timeout => THEME_ERROR,
    }
}

/// 连通目标的展示文案与颜色：有中位数给毫秒，测量中提示，全部失败给「超时」。
pub(crate) fn latency_display(target: &LatencyState) -> (String, Color) {
    match target.median {
        Some(ms) => (format!("{ms}ms"), latency_color(tier(Some(ms)))),
        None if target.done => ("超时".to_string(), THEME_ERROR),
        None => ("测量中…".to_string(), THEME_MUTED),
    }
}
