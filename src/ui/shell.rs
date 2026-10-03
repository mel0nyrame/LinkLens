//! 应用外壳：标题行、页面导航、内容区与键位提示的整体布局。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::app::{App, Page};
use crate::state::AppState;
use crate::theme::color::{THEME_ACCENT, THEME_MUTED, THEME_WARNING};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card};
use crate::ui::layout::card_grid_columns;
use crate::ui::pages;

/// 渲染整个应用外壳（对共享状态做快照读取）。
pub fn render(f: &mut Frame, state: &AppState) {
    let [title_bar, tabs_bar, content, help_bar] = shell_areas(f.area());

    render_title(f, title_bar, &state.update);
    render_tabs(f, tabs_bar, &state.app);
    render_page(f, content, state);
    render_help(f, help_bar, &state.app);
    render_update_dialog(f, &state.update);
}

fn shell_areas(area: Rect) -> [Rect; 4] {
    let [title, tabs, _, content, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    [title, tabs, content, help]
}

pub fn content_area(area: Rect) -> Rect {
    shell_areas(area)[2]
}

fn render_title(f: &mut Frame, area: Rect, update: &crate::state_update::UpdateState) {
    let mut spans = vec![Span::styled(
        icon::labeled(Icon::Terminal, "LinkLens"),
        Style::new().fg(THEME_ACCENT),
    )];
    if update.downloading {
        spans.push(Span::styled(
            " · 正在下载并校验… Esc 取消",
            Style::new().fg(THEME_WARNING),
        ));
    } else if let Some(available) = &update.available {
        spans.push(Span::styled(
            format!(" · 新版 {} · u 更新", available.version()),
            Style::new().fg(THEME_WARNING),
        ));
    }
    if let Some(notice) = &update.notice {
        spans.push(Span::styled(
            format!(" · {notice}"),
            Style::new().fg(THEME_MUTED),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_update_dialog(f: &mut Frame, update: &crate::state_update::UpdateState) {
    if !update.dialog {
        return;
    }
    let area = f.area();
    let width = area.width.min(66);
    let height = area.height.min(10);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, popup);
    let block = card("LinkLens 更新");
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let lines = if let Some(error) = &update.error {
        vec![
            Line::styled(
                format!("更新失败：{error}"),
                Style::new().fg(crate::theme::color::THEME_ERROR),
            ),
            Line::from(""),
            Line::from("Esc / Enter 返回当前页面"),
        ]
    } else {
        let version = update
            .available
            .as_ref()
            .map(|available| available.version())
            .unwrap_or("");
        vec![
            Line::from(format!(
                "当前版本 {} → 新版 {}",
                env!("CARGO_PKG_VERSION"),
                version
            )),
            Line::from("下载并校验两个命令；安装完成后退出，请重新启动。"),
            Line::from(""),
            Line::from(vec![
                if update.select_update {
                    badge("更新", THEME_ACCENT)
                } else {
                    Span::raw(" 更新 ")
                },
                Span::raw("  "),
                if !update.select_update {
                    badge("稍后", THEME_ACCENT)
                } else {
                    Span::raw(" 稍后 ")
                },
            ]),
            Line::styled(
                "方向键 / Tab 选择 · Enter 确认 · Esc 返回",
                Style::new().fg(THEME_MUTED),
            ),
        ]
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
    if state.app.page == Page::IpScore {
        pages::ip_score::render(f, area, &state.score, state.app.hide_ip);
    } else if !area.is_empty() {
        let buffer = pages::canvas(area.width, state);
        crate::ui::scroll::copy_viewport(&buffer, f.buffer_mut(), area, state.app.scroll_offset());
    }
}

fn render_help(f: &mut Frame, area: Rect, app: &App) {
    let help = if app.page == Page::IpScore {
        "←→/1-7 · / 输入 · [ ] 最近 · Enter 查 · ↑↓ PgUp/PgDn Home/End · i 打码 · q 退出"
    } else {
        "←→/1-7 换页 · ↑↓ PgUp/PgDn Home/End · i 打码 · r 重查 · q 退出"
    };
    let mut spans = vec![Span::styled(help, Style::new().fg(THEME_MUTED))];
    if app.hide_ip {
        spans.push(Span::styled(" · 已隐藏 IP", Style::new().fg(THEME_WARNING)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_notice_and_modal_render_within_narrow_terminal() {
        let mut state = AppState::default();
        state.update.available = Some(
            crate::update::AvailableUpdate::for_release("v0.2.0", "aarch64-apple-darwin").unwrap(),
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| render(f, &state)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains("新版0.2.0")
        );
        state.update.dialog = true;
        terminal.draw(|f| render(f, &state)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains("LinkLens更新")
        );
        assert!(
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains("稍后")
        );
    }
}
