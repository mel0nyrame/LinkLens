//! WebRTC 泄漏页（空壳）。

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::theme::icon::{self, Icon};

use super::placeholder;

pub fn render(f: &mut Frame, area: Rect) {
    placeholder(f, area, icon::labeled(Icon::WebRtc, "WebRTC"));
}
