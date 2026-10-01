//! GPT 检测页：共用 AI 框架的 GPT 参数化实例（出口源/探测目标/状态路径三处不同）。

use ratatui::buffer::Buffer;

use crate::probe_ai::GPT_PROFILE;
use crate::state::AppState;

use super::ai;

pub fn canvas(width: u16, state: &AppState) -> Buffer {
    ai::canvas(width, &GPT_PROFILE, &state.ai.gpt, state.app.hide_ip)
}
