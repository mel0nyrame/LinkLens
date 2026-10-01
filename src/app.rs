//! 应用状态与键位导航：页面切换、隐藏 IP 开关、退出。
//!
//! 键位约定：`1`-`7` 直达页面，`←`/`→`（或 `h`/`l`、`Tab`/`Shift+Tab`）顺序循环，
//! `i` 切换隐藏 IP 打码，`q`/`Esc`/`Ctrl+C` 退出。带输入框的页面在后续票中
//! 先行消费按键，再回落到此层。隐藏 IP 是全局开关：影响所有页面的 IP 显示。

use crossterm::event::{KeyCode, KeyModifiers};
use futures_util::StreamExt;
use ratatui::crossterm::event::{Event, EventStream, KeyEventKind};

use crate::theme::icon::Icon;
use crate::ui;
use crate::{probe, state};

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

    /// 页面在导航与卡片标题中使用的图标。
    pub fn icon(self) -> Icon {
        match self {
            Page::IpQuery => Icon::IpQuery,
            Page::Claude => Icon::Claude,
            Page::Gpt => Icon::Gpt,
            Page::IpScore => Icon::IpScore,
            Page::DnsLeak => Icon::DnsLeak,
            Page::WebRtc => Icon::WebRtc,
            Page::Connectivity => Icon::Connectivity,
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

/// 应用状态：当前页面、隐藏 IP 开关与退出标记。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct App {
    pub page: Page,
    /// 隐藏 IP 开关：开启后界面所有 IP 打码显示（方便截图分享）。
    pub hide_ip: bool,
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
            KeyCode::Char('i') => self.hide_ip = !self.hide_ip,
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

/// 运行终端应用：初始化终端、启动后台探测、事件循环、恢复终端。
///
/// 无论循环以何种方式结束（正常退出、绘制或读取事件失败）都会恢复终端状态；
/// panic 时的恢复由 `ratatui::try_init` 安装的钩子负责。
/// 探测任务通过共享状态 + 通知驱动重绘：键位事件与数据更新都会唤醒循环。
pub async fn run() -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mut events = EventStream::new();
    let shared = state::SharedState::new();
    probe::spawn_home(shared.clone());
    let mut link_spawned = false;

    let result = loop {
        // 首次进入连通页时启动 47 目标测量（仅一次）
        if !link_spawned && shared.lock().app.page == Page::Connectivity {
            probe::spawn_link(shared.clone());
            link_spawned = true;
        }

        {
            let snapshot = shared.lock();
            if let Err(err) = terminal.draw(|f| ui::shell::render(f, &snapshot)) {
                break Err(err);
            }
        }

        tokio::select! {
            event = events.next() => {
                match event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        shared.lock().app.handle_key(key.code, key.modifiers);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => break Err(err),
                    None => break Ok(()),
                }
            }
            _ = shared.changed() => {}
        }
    };

    let _ = ratatui::try_restore();
    result
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
            [
                "IP 查询",
                "Claude 检测",
                "GPT 检测",
                "IP 评分",
                "DNS 泄漏",
                "WebRTC",
                "网络连通"
            ]
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
        let mut app = App {
            page: Page::Connectivity,
            ..App::default()
        };
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

    #[test]
    fn i_key_toggles_ip_masking_without_changing_page() {
        let mut app = App::default();
        assert!(!app.hide_ip, "默认不打码");
        app.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);
        assert!(app.hide_ip);
        assert_eq!(app.page, Page::IpQuery);
        assert!(!app.should_quit);
        app.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);
        assert!(!app.hide_ip);
    }
}
