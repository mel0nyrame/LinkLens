//! GPT 检测页：共用 AI 框架的 GPT 参数化实例（出口源/探测目标/状态路径三处不同）。

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::probe_ai::GPT_PROFILE;
use crate::state::AppState;

use super::ai;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    ai::render(
        f,
        area,
        &GPT_PROFILE,
        &state.ai.gpt,
        &state.home,
        state.app.hide_ip,
    );
}
