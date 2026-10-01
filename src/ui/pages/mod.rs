//! 七个功能页。每页只导出一个 `render`；页面内容由后续各票填充。

pub mod ai;
pub mod claude;
pub mod connectivity;
pub mod dns_leak;
pub mod gpt;
pub mod ip_query;
pub mod ip_score;
pub mod webrtc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::net::latency::{LatencyTier, tier};
use crate::state::LatencyState;
use crate::theme::color::{
    THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_SUCCESS_SOFT, THEME_WARNING,
};
use crate::theme::widget::card;

/// 页面空壳统一占位：标题卡片 + 建设中提示。
pub(crate) fn placeholder(f: &mut Frame, area: Rect, title: String) {
    let block = card(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let hint = Line::styled("功能建设中，将在后续版本提供", Style::new().fg(THEME_MUTED));
    f.render_widget(Paragraph::new(hint), inner);
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
