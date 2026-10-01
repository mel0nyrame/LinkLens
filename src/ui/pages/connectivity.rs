//! 网络连通页：目标清单按 cn/jp/us/全球四组的全量连通表。
//!
//! 每行 = 目标名 + 8 轮状态色点 + 中位延迟（分档色，失败「超时」）；
//! 组头给出可达数与组内中位延迟平均；与首页 6 目标小卡共用同一计时工具
//! （`net::latency`，预热 1 次 + 8 轮取中位数）。
//!
//! 布局：宽 ≥90 列时四组两两并排（2×2），窄终端单列纵排，超出部分通过整页滚动查看。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::net::latency::RoundPlan;
use crate::net::targets::{GROUPS, TARGETS};
use crate::state::{LatencyState, LinkState};
use crate::theme::color::{THEME_MUTED, THEME_TEXT};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::card;

use super::latency_display;

/// 完整分组画布；宽屏两列、窄屏单列，保留清单全部目标。
pub fn canvas(width: u16, link: &LinkState) -> Buffer {
    if !link.started {
        return crate::ui::scroll::card_canvas(
            width,
            vec![
                Paragraph::new(format!("进入本页即开始 {} 目标连通测量…", TARGETS.len()))
                    .block(card(icon::labeled(Icon::Connectivity, "网络连通"))),
            ],
        );
    }
    let columns = if width >= crate::ui::layout::GRID_MIN_WIDTH {
        2
    } else {
        1
    };
    let column_width = width.saturating_sub(columns - 1) / columns;
    let cards: Vec<_> = GROUPS
        .iter()
        .enumerate()
        .map(|(i, group)| group_card(column_width, i, group.flag, group.name, link))
        .collect();
    let mut y = 1u16;
    let mut rects = Vec::new();
    for row in cards.chunks(usize::from(columns)) {
        let height = row
            .iter()
            .map(|p| p.line_count(column_width.saturating_sub(4).max(1)))
            .max()
            .unwrap_or(2)
            .min(usize::from(u16::MAX)) as u16;
        for col in 0..row.len() {
            rects.push(Rect::new(
                col as u16 * (column_width + 1),
                y,
                column_width,
                height,
            ));
        }
        y = y.saturating_add(height).saturating_add(1);
    }
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, y.saturating_sub(1)));
    let completed = link.targets.iter().filter(|t| t.done).count();
    Paragraph::new(format!(
        "第 {} 次测量 · {}/{} 已完成 · r 从头重测",
        link.generation,
        completed,
        TARGETS.len()
    ))
    .style(Style::new().fg(THEME_TEXT))
    .render(Rect::new(0, 0, width, 1), &mut buffer);
    for (paragraph, rect) in cards.into_iter().zip(rects) {
        paragraph.render(rect, &mut buffer);
    }
    buffer
}

fn group_card(
    width: u16,
    group_index: usize,
    flag: &str,
    name: &str,
    link: &LinkState,
) -> Paragraph<'static> {
    let targets: Vec<(usize, &LatencyState)> = TARGETS
        .iter()
        .enumerate()
        .filter(|(_, t)| t.group.index() == group_index)
        .filter_map(|(i, _)| link.targets.get(i).map(|s| (i, s)))
        .collect();
    let (reachable, avg) = group_summary(&targets);
    let title = icon::labeled(
        Icon::Earth,
        &format!(
            "{flag} {name} · 可达 {reachable}/{} · 平均 {avg}",
            targets.len()
        ),
    );
    let lines = targets
        .iter()
        .map(|(_, target)| target_row(width.saturating_sub(4), target))
        .collect::<Vec<_>>();
    Paragraph::new(lines)
        .block(card(title))
        .wrap(Wrap { trim: false })
}

/// 名称列按显示宽度对齐，保留 8 轮色点和延迟。
fn target_row(width: u16, target_state: &LatencyState) -> Line<'static> {
    let (text, color) = latency_display(target_state);
    let name_width = usize::from(width.saturating_sub(RoundPlan::LINK.rounds as u16 + 10));
    let name = truncate_name(target_state.name, name_width);
    let padding = name_width.saturating_sub(Line::raw(&name).width());
    let mut spans = vec![Span::styled(
        format!("{name}{} ", " ".repeat(padding)),
        Style::new().fg(THEME_TEXT),
    )];
    spans.extend(round_dots(target_state));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(text, Style::new().fg(color).bold()));
    Line::from(spans)
}

/// 按终端显示宽度截断名称，省略号占一列。
fn truncate_name(name: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Line::raw(name).width() <= width {
        return name.to_string();
    }
    let mut cut = String::new();
    for c in name.chars() {
        let next_width = Line::raw(format!("{cut}{c}")).width();
        if next_width >= width {
            break;
        }
        cut.push(c);
    }
    format!("{cut}…")
}

/// 8 轮色点：已测轮按当轮分档取色，未测轮暗灰小点。
fn round_dots(target_state: &LatencyState) -> Vec<Span<'static>> {
    use crate::net::latency::tier as round_tier;

    let total = RoundPlan::LINK.rounds as usize;
    (0..total)
        .map(|i| match target_state.rounds.get(i) {
            Some(result) => Span::styled(
                "●",
                Style::new().fg(super::latency_color(round_tier(*result))),
            ),
            None => Span::styled("·", Style::new().fg(THEME_MUTED)),
        })
        .collect()
}

/// 组内汇总：可达数 + 成功目标中位延迟的平均（无成功目标为「--」）。
fn group_summary(targets: &[(usize, &LatencyState)]) -> (usize, String) {
    let reachable = targets.iter().filter(|(_, s)| s.median.is_some()).count();
    let meds: Vec<u64> = targets.iter().filter_map(|(_, s)| s.median).collect();
    if meds.is_empty() {
        return (reachable, "--".to_string());
    }
    let avg = meds.iter().sum::<u64>() / meds.len() as u64;
    (reachable, format!("{avg}ms"))
}

#[cfg(test)]
mod tests {
    use super::truncate_name;

    #[test]
    fn chinese_target_names_fit_their_terminal_columns() {
        assert_eq!(truncate_name("中国电信", 5), "中国…");
        assert_eq!(truncate_name("中国电信", 8), "中国电信");
        assert_eq!(truncate_name("Cloudflare", 8), "Cloudfl…");
        assert_eq!(truncate_name("中国电信", 0), "");
    }
}
