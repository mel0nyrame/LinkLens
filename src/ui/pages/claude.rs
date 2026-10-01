//! Claude 检测页：共用 AI 框架的 Claude 参数化实例。

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::probe_ai::CLAUDE_PROFILE;
use crate::state::AppState;

use super::ai;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    ai::render(
        f,
        area,
        &CLAUDE_PROFILE,
        &state.ai.claude,
        &state.home,
        state.app.hide_ip,
    );
}
