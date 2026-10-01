//! 应用外壳：标题行、页面导航、内容区与键位提示的整体布局。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Page};
use crate::state::AppState;
use crate::theme::color::{THEME_ACCENT, THEME_MUTED, THEME_WARNING};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::badge;
use crate::ui::layout::card_grid_columns;
use crate::ui::pages;

/// 渲染整个应用外壳（对共享状态做快照读取）。
pub fn render(f: &mut Frame, state: &AppState) {
    let [title_bar, tabs_bar, content, help_bar] = shell_areas(f.area());

    render_title(f, title_bar);
    render_tabs(f, tabs_bar, &state.app);
    render_page(f, content, state);
    render_help(f, help_bar, &state.app);
}

fn shell_areas(area: Rect) -> [Rect; 4] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area)
}

pub fn content_area(area: Rect) -> Rect {
    shell_areas(area)[2]
}

fn render_title(f: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(
        icon::labeled(Icon::Terminal, "LinkLens"),
        Style::new().fg(THEME_ACCENT),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    // 窄终端降级单列时隐藏导航图标，避免标签行溢出
    let with_icons = card_grid_columns(f.area().width) >= 3;
    let spans = Page::ALL
        .iter()
        .map(|page| {
            let label = if with_icons {
                icon::labeled(page.icon(), page.title())
            } else {
                page.title().to_string()
            };
            if *page == app.page {
                badge(&label, THEME_ACCENT)
            } else {
                Span::styled(format!(" {label} "), Style::new().fg(THEME_MUTED))
            }
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_page(f: &mut Frame, area: Rect, state: &AppState) {
    match state.app.page {
        Page::IpQuery => pages::ip_query::render(f, area, &state.home, state.app.hide_ip),
        Page::Claude => pages::claude::render(f, area, state),
        Page::Gpt => pages::gpt::render(f, area, state),
        Page::IpScore => pages::ip_score::render(f, area, &state.score, state.app.hide_ip),
        Page::DnsLeak => pages::dns_leak::render(f, area, &state.dns_leak, state.app.hide_ip),
        Page::WebRtc => pages::webrtc::render(f, area, &state.webrtc, state.app.hide_ip),
        Page::Connectivity => pages::connectivity::render(f, area, &state.link),
    }
}

fn render_help(f: &mut Frame, area: Rect, app: &App) {
    let help = if app.page == Page::IpScore {
        "Tab/←→ 换页 · i 隐藏 IP · / 编辑 · Enter 查询 · r 重查 · ↑↓ 滚动 · q/Ctrl+C 退出"
    } else {
        "←/→ 或 h/l 顺序切换 · 1-7 直达页面 · i 切换隐藏 IP · r 重查当前 AI 检测页 · q/Esc/Ctrl+C 退出"
    };
    let mut spans = vec![Span::styled(help, Style::new().fg(THEME_MUTED))];
    if app.hide_ip {
        spans.push(Span::styled(" · 已隐藏 IP", Style::new().fg(THEME_WARNING)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
