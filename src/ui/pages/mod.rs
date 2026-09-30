//! 七个功能页。每页只导出一个 `render`；页面内容由后续各票填充。

pub mod claude;
pub mod connectivity;
pub mod dns_leak;
pub mod gpt;
pub mod ip_query;
pub mod ip_score;
pub mod webrtc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::theme::color::THEME_MUTED;
use crate::theme::widget::card;

/// 页面空壳统一占位：标题卡片 + 建设中提示。
pub(crate) fn placeholder(f: &mut Frame, area: Rect, title: String) {
    let block = card(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let hint = Line::styled("功能建设中，将在后续版本提供", Style::new().fg(THEME_MUTED));
    f.render_widget(Paragraph::new(hint), inner);
}
