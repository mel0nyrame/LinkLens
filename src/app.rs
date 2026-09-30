//! 应用状态与键位导航：页面切换、退出。
//!
//! 键位约定：`1`-`7` 直达页面，`←`/`→`（或 `h`/`l`、`Tab`/`Shift+Tab`）顺序循环，
//! `q`/`Esc`/`Ctrl+C` 退出。带输入框的页面在后续票中先行消费按键，再回落到此层。

use crossterm::event::{KeyCode, KeyModifiers};

/// 七个功能页，顺序即导航顺序，也是数字键 `1`-`7` 的直达目标。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    IpQuery,
    Claude,
    Gpt,
    IpScore,
    DnsLeak,
    WebRtc,
    Connectivity,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::IpQuery,
        Page::Claude,
        Page::Gpt,
        Page::IpScore,
        Page::DnsLeak,
        Page::WebRtc,
        Page::Connectivity,
    ];

    /// 页面标题（导航栏与卡片标题共用）。
    pub fn title(self) -> &'static str {
        match self {
            Page::IpQuery => "IP 查询",
            Page::Claude => "Claude 检测",
            Page::Gpt => "GPT 检测",
            Page::IpScore => "IP 评分",
            Page::DnsLeak => "DNS 泄漏",
            Page::WebRtc => "WebRTC",
            Page::Connectivity => "网络连通",
        }
    }

    /// 数字键 `1`-`7` 直达；其余返回 `None`。
    pub fn from_digit(digit: u8) -> Option<Page> {
        let index = usize::from(digit).checked_sub(1)?;
        Page::ALL.get(index).copied()
    }

    fn position(self) -> usize {
        Page::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    pub fn next(self) -> Page {
        Page::ALL[(self.position() + 1) % Page::ALL.len()]
    }

    pub fn previous(self) -> Page {
        let len = Page::ALL.len();
        Page::ALL[(self.position() + len - 1) % len]
    }
}

/// 应用状态：当前页面与退出标记。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct App {
    pub page: Page,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> App {
        App::default()
    }

    /// 全局键位处理；不带输入焦点的假设下由事件循环直接调用。
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if modifiers == KeyModifiers::CONTROL => self.should_quit = true,
            KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                self.page = self.page.previous();
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.page = self.page.next(),
            KeyCode::Char(d @ '1'..='7') => {
                if let Some(page) = Page::from_digit(d.to_digit(10).unwrap_or(0) as u8) {
                    self.page = page;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, Page};
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn seven_pages_in_fixed_order() {
        assert_eq!(Page::ALL.len(), 7);
        let titles: Vec<_> = Page::ALL.iter().map(|p| p.title()).collect();
        assert_eq!(
            titles,
            ["IP 查询", "Claude 检测", "GPT 检测", "IP 评分", "DNS 泄漏", "WebRTC", "网络连通"]
        );
    }

    #[test]
    fn digit_keys_1_to_7_jump_to_pages() {
        for (i, page) in Page::ALL.iter().enumerate() {
            assert_eq!(Page::from_digit((i + 1) as u8), Some(*page));
        }
        assert_eq!(Page::from_digit(0), None);
        assert_eq!(Page::from_digit(8), None);
    }

    #[test]
    fn right_and_left_cycle_pages() {
        let mut app = App::default();
        assert_eq!(app.page, Page::IpQuery);

        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.page, Page::Claude);

        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.page, Page::IpQuery);

        // 首页向左循环到末页
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.page, Page::Connectivity);
    }

    #[test]
    fn last_page_right_cycles_back_to_first() {
        let mut app = App::default();
        app.page = Page::Connectivity;
        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.page, Page::IpQuery);
    }

    #[test]
    fn tab_and_backtab_cycle_pages() {
        let mut app = App::default();
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.page, Page::Claude);
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(app.page, Page::IpQuery);
    }

    #[test]
    fn digit_key_sets_target_page() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);
        assert_eq!(app.page, Page::DnsLeak);
    }

    #[test]
    fn q_esc_and_ctrl_c_quit() {
        for (code, mods) in [
            (KeyCode::Char('q'), KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut app = App::default();
            app.handle_key(code, mods);
            assert!(app.should_quit, "{code:?} 应退出");
        }
    }

    #[test]
    fn other_keys_do_not_quit_or_navigate() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.page, Page::IpQuery);
    }
}
