//! Claude 检测页：共用 AI 框架的 Claude 参数化实例。

use ratatui::buffer::Buffer;

use crate::probe_ai::CLAUDE_PROFILE;
use crate::state::AppState;

use super::ai;

pub fn canvas(width: u16, state: &AppState) -> Buffer {
    ai::canvas(width, &CLAUDE_PROFILE, &state.ai.claude, state.app.hide_ip)
}
