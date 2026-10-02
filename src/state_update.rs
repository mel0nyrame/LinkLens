//! TUI 的版本提示、用户确认与下载状态；不负责网络或安装。
use crate::update::AvailableUpdate;
use crossterm::event::{KeyCode, KeyModifiers};

#[derive(Default)]
pub struct UpdateState {
    pub available: Option<AvailableUpdate>,
    pub dialog: bool,
    pub select_update: bool,
    pub downloading: bool,
    pub error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UpdateAction {
    None,
    Download,
    Cancel,
    Quit,
}

impl UpdateState {
    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Option<UpdateAction> {
        if (self.dialog || self.downloading)
            && (code == KeyCode::Char('q')
                || (code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL)))
        {
            return Some(UpdateAction::Quit);
        }
        if self.downloading {
            return match code {
                KeyCode::Esc => {
                    self.downloading = false;
                    self.dialog = false;
                    Some(UpdateAction::Cancel)
                }
                KeyCode::Char('u') | KeyCode::Enter => Some(UpdateAction::None),
                _ => None,
            };
        }
        if self.dialog {
            match code {
                KeyCode::Esc => self.dialog = false,
                KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                    if self.error.is_none() =>
                {
                    self.select_update = !self.select_update
                }
                KeyCode::Enter if self.select_update && self.error.is_none() => {
                    self.dialog = false;
                    self.downloading = true;
                    return Some(UpdateAction::Download);
                }
                KeyCode::Enter => self.dialog = false,
                _ => {}
            }
            return Some(UpdateAction::None);
        }
        if code == KeyCode::Char('u') && self.available.is_some() {
            self.dialog = true;
            self.select_update = false;
            self.error = None;
            return Some(UpdateAction::None);
        }
        None
    }
    pub fn failed(&mut self, error: String) {
        self.downloading = false;
        self.dialog = true;
        self.select_update = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_dialog_defaults_to_later_and_escape_preserves_notice() {
        let mut state = UpdateState {
            available: Some(
                AvailableUpdate::for_release("v0.2.0", "aarch64-apple-darwin").unwrap(),
            ),
            ..UpdateState::default()
        };
        assert_eq!(
            state.handle_key(KeyCode::Char('u'), KeyModifiers::NONE),
            Some(UpdateAction::None)
        );
        assert!(state.dialog);
        assert!(!state.select_update);
        assert_eq!(
            state.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            Some(UpdateAction::None)
        );
        assert!(!state.dialog);
        state.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);
        state.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(!state.dialog);
        assert!(state.available.is_some());
    }
    #[test]
    fn download_requires_selection_and_can_be_cancelled_without_leaving_page() {
        let mut state = UpdateState {
            available: Some(
                AvailableUpdate::for_release("v0.2.0", "aarch64-apple-darwin").unwrap(),
            ),
            ..UpdateState::default()
        };
        state.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);
        state.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            state.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            Some(UpdateAction::Download)
        );
        assert!(state.downloading);
        assert_eq!(
            state.handle_key(KeyCode::Char('u'), KeyModifiers::NONE),
            Some(UpdateAction::None)
        );
        assert_eq!(
            state.handle_key(KeyCode::Esc, KeyModifiers::NONE),
            Some(UpdateAction::Cancel)
        );
        assert!(!state.downloading);
        assert!(!state.dialog);
        assert!(state.available.is_some());
    }
}
