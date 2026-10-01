//! 应用状态与输入导航：页面切换、隐藏 IP 开关、重查、滚动与退出。
//!
//! 键位约定：`1`-`7` 直达页面，`←`/`→`（或 `h`/`l`、`Tab`/`Shift+Tab`）顺序循环，
//! `i` 切换隐藏 IP 打码，`r` 在首页、Claude/GPT 和网络连通页重查，`q`/`Esc`/`Ctrl+C` 退出。
//! 评分页输入焦点先行消费按键，再回落到此层。
//! 隐藏 IP 是全局开关：影响所有页面的 IP 显示。

use crossterm::event::{KeyCode, KeyModifiers};
use futures_util::StreamExt;
use ratatui::crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind, MouseEventKind,
};

use crate::theme::icon::Icon;
use crate::ui;
use crate::{probe, probe_ai, probe_leak, probe_score, state};

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

/// 应用状态：当前页面、隐藏 IP 开关、页面重查请求与退出标记。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct App {
    pub page: Page,
    /// 隐藏 IP 开关：开启后界面所有 IP 打码显示（方便截图分享）。
    pub hide_ip: bool,
    /// `r` 键置位的重查请求（首页、Claude/GPT 与网络连通页），事件循环消费后复位。
    pub refresh_requested: bool,
    pub should_quit: bool,
    /// 各页独立保存垂直位置；评分页由其输入与查询状态持有滚动位置。
    pub scroll: [u16; 7],
}

impl App {
    pub fn new() -> App {
        App::default()
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll[self.page.position()]
    }

    pub fn clamp_scroll(&mut self, max: u16) {
        let scroll = &mut self.scroll[self.page.position()];
        *scroll = (*scroll).min(max);
    }

    pub fn handle_scroll(&mut self, key: KeyCode, max: u16) -> bool {
        if let Some(offset) = ui::scroll::key_scroll(self.scroll_offset(), key, max) {
            self.scroll[self.page.position()] = offset;
            true
        } else {
            false
        }
    }

    /// 全局键位处理；不带输入焦点的假设下由事件循环直接调用。
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Char('i') => self.hide_ip = !self.hide_ip,
            KeyCode::Char('r')
                if matches!(
                    self.page,
                    Page::IpQuery | Page::Claude | Page::Gpt | Page::Connectivity
                ) =>
            {
                self.refresh_requested = true;
            }
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

struct MouseCapture;

impl Drop for MouseCapture {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
}

/// 运行终端应用；输入事件与数据更新唤醒重绘，退出与 panic 时恢复终端和鼠标模式。
pub async fn run() -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let _mouse_capture = MouseCapture;
    if let Err(error) = crossterm::execute!(std::io::stdout(), EnableMouseCapture) {
        let _ = ratatui::try_restore();
        return Err(error);
    }
    // 鼠标移动、拖动与点击没有页面行为，须在唤醒重绘前丢弃，避免连续移动积压滚轮。
    let mut events = EventStream::new().filter(|event| {
        std::future::ready(!matches!(
            event,
            Ok(Event::Mouse(mouse))
                if !matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown)
        ))
    });
    let shared = state::SharedState::new();
    let mut home_task = probe::spawn_home(shared.clone());
    let mut link_task = None;
    let mut webrtc_spawned = false;

    let result = loop {
        if shared.lock().app.should_quit {
            break Ok(());
        }
        // 首次进入连通页时启动全部目标测量。
        let start_link = {
            let snapshot = shared.lock();
            snapshot.app.page == Page::Connectivity && !snapshot.link.started
        };
        if start_link {
            link_task = probe::spawn_link(shared.clone(), false);
        }
        // 首次进入 WebRTC 页时启动 STUN 探测（仅一次）
        if !webrtc_spawned && shared.lock().app.page == Page::WebRtc {
            webrtc_spawned = true;
            probe_leak::spawn_webrtc_probe(shared.clone());
        }

        // 首次进入 Claude/GPT 页时启动对应检测（幂等）
        probe_ai::spawn_for_page_if_needed(shared.clone());
        probe_score::spawn_for_page_if_needed(shared.clone());

        // `r` 键重查当前首页、AI 或连通页
        let refresh_page = {
            let mut snapshot = shared.lock();
            let page = snapshot.app.page;
            if snapshot.app.refresh_requested {
                snapshot.app.refresh_requested = false;
                Some(page)
            } else {
                None
            }
        };
        if let Some(page) = refresh_page {
            if page == Page::IpQuery {
                home_task.abort();
                home_task = probe::spawn_home(shared.clone());
            } else if page == Page::Connectivity {
                if let Some(task) = link_task.take() {
                    task.abort();
                }
                link_task = probe::spawn_link(shared.clone(), true);
            } else {
                probe_ai::request_refresh(&shared, page);
            }
        }

        {
            let mut snapshot = shared.lock();
            if snapshot.app.page == Page::IpScore {
                let content = ui::shell::content_area(terminal.get_frame().area());
                snapshot.score.scroll = snapshot.score.scroll.min(ui::pages::ip_score::max_scroll(
                    content,
                    &snapshot.score,
                    snapshot.app.hide_ip,
                ));
            } else {
                let content = ui::shell::content_area(terminal.get_frame().area());
                let max = ui::pages::max_scroll(content, &snapshot);
                snapshot.app.clamp_scroll(max);
            }
            if let Err(err) = terminal.draw(|f| ui::shell::render(f, &snapshot)) {
                break Err(err);
            }
        }

        tokio::select! {
            event = events.next() => {
                match event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        let consumed = {
                            let mut snapshot = shared.lock();
                            if snapshot.app.page == Page::IpScore {
                                let content = ui::shell::content_area(terminal.get_frame().area());
                                let max_scroll = ui::pages::ip_score::max_scroll(content, &snapshot.score, snapshot.app.hide_ip);
                                snapshot.score.handle_key(key.code, key.modifiers, max_scroll)
                            } else {
                                let content = ui::shell::content_area(terminal.get_frame().area());
                                let max = ui::pages::max_scroll(content, &snapshot);
                                snapshot.app.handle_scroll(key.code, max)
                            }
                        };
                        if !consumed {
                            let page = shared.lock().app.page;
                            probe_leak::handle_page_key(shared.clone(), page, key.code);
                            shared.lock().app.handle_key(key.code, key.modifiers);
                        }
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        let mut snapshot = shared.lock();
                        let content = ui::shell::content_area(terminal.get_frame().area());
                        if snapshot.app.page == Page::IpScore {
                            let max = ui::pages::ip_score::max_scroll(content, &snapshot.score, snapshot.app.hide_ip);
                            if let Some(scroll) = ui::scroll::mouse_scroll(snapshot.score.scroll, mouse.kind, max) {
                                snapshot.score.scroll = scroll;
                            }
                        } else {
                            let max = ui::pages::max_scroll(content, &snapshot);
                            if let Some(scroll) = ui::scroll::mouse_scroll(snapshot.app.scroll_offset(), mouse.kind, max) {
                                let page = snapshot.app.page.position();
                                snapshot.app.scroll[page] = scroll;
                            }
                        }
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
    fn regression_home_r_requests_a_new_probe() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
        assert!(app.refresh_requested, "IP 查询页 r 必须提交重查请求");
    }

    #[test]
    fn regression_connectivity_r_requests_a_new_probe() {
        let mut app = App {
            page: Page::Connectivity,
            ..App::default()
        };
        app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
        assert!(app.refresh_requested, "网络连通页 r 必须提交重测请求");
    }

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

    #[test]
    fn r_key_requests_refresh_on_supported_pages() {
        for page in [Page::IpQuery, Page::Claude, Page::Gpt, Page::Connectivity] {
            let mut app = App {
                page,
                ..App::default()
            };
            app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
            assert!(app.refresh_requested, "{page:?} 页 r 应请求重查");
        }
        let mut app = App {
            page: Page::DnsLeak,
            ..App::default()
        };
        app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
        assert!(!app.refresh_requested);
        assert!(!app.should_quit);
        assert_eq!(app.page, Page::DnsLeak);
    }
}
