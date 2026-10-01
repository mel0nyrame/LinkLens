//! 网络连通页：47 目标按 cn/jp/us/全球四组的全量连通表。
//!
//! 每行 = 目标名 + 12 轮状态色点 + 中位延迟（分档色，失败「超时」）；
//! 组头给出可达数与组内中位延迟平均；与首页 6 目标小卡共用同一计时工具
//! （`net::latency`，预热 1 次 + 12 轮取中位数）。
//!
//! 布局：宽 ≥90 列时四组两两并排（2×2），窄终端单列纵排（超出部分裁剪）。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::net::latency::RoundPlan;
use crate::net::targets::{GROUPS, GroupId, TARGETS};
use crate::state::{LatencyState, LinkState};
use crate::theme::color::{THEME_MUTED, THEME_TEXT};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::card;

use super::latency_display;

/// 渲染网络连通页。
pub fn render(f: &mut Frame, area: Rect, link: &LinkState) {
    if !link.started {
        let block = card(icon::labeled(Icon::Connectivity, "网络连通"));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(Line::styled(
                "进入本页即开始 47 目标连通测量…",
                Style::new().fg(THEME_MUTED),
            )),
            inner,
        );
        return;
    }

    // 组行高：按各组最大行数预留（每组标题 1 行 + 目标行）
    let rows_per_group = [
        group_len(GroupId::Cn),
        group_len(GroupId::Jp),
        group_len(GroupId::Us),
        group_len(GroupId::Global),
    ];
    let band_heights = [
        rows_per_group[0].max(rows_per_group[1]) as u16 + 1,
        rows_per_group[2].max(rows_per_group[3]) as u16 + 1,
    ];
    let wide = area.width >= crate::ui::layout::GRID_MIN_WIDTH;

    let (top_rects, bottom_rects) = if wide {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(band_heights[0]), Constraint::Min(0)]).areas(area);
        let top_rects: [Rect; 2] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(top);
        let bottom_rects: [Rect; 2] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(bottom);
        (top_rects, bottom_rects)
    } else {
        let [g0, g1, g2, g3] = Layout::vertical([
            Constraint::Length(band_heights[0]),
            Constraint::Length(band_heights[1]),
            Constraint::Length(band_heights[0]),
            Constraint::Length(band_heights[1]),
        ])
        .areas(area);
        ([g0, g1], [g2, g3])
    };

    for (group_index, group) in GROUPS.iter().enumerate() {
        let rect = if group_index < 2 {
            top_rects[group_index]
        } else {
            bottom_rects[group_index - 2]
        };
        render_group(f, rect, group_index, group.flag, group.name, link);
    }
}

/// 渲染一组：组卡（标题含国旗与汇总）+ 目标行。
fn render_group(
    f: &mut Frame,
    area: Rect,
    group_index: usize,
    flag: &str,
    name: &str,
    link: &LinkState,
) {
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
    let block = card(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // 可显示行数受内区高度限制（超出裁剪，不滚动）
    let capacity = usize::from(inner.height);
    for (row, (_, target_state)) in targets.iter().take(capacity).enumerate() {
        let row_area = Rect {
            x: inner.x,
            y: inner.y.saturating_add(row as u16),
            width: inner.width,
            height: 1,
        };
        render_target_row(f, row_area, target_state);
    }
}

/// 单目标行：名称（截断）+ 12 轮色点 + 中位延迟。
fn render_target_row(f: &mut Frame, area: Rect, target_state: &LatencyState) {
    let (text, color) = latency_display(target_state);
    // 名称列按终端宽度弹性分配：点 12 列 + 延迟 7 列，其余给名称
    let name_width = usize::from(area.width.saturating_sub(RoundPlan::HOME.rounds as u16 + 9));
    let name = truncate_name(target_state.name, name_width);
    let mut spans = vec![Span::styled(
        format!("{name:<name_width$} "),
        Style::new().fg(THEME_TEXT),
    )];
    spans.extend(round_dots(target_state));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(text, Style::new().fg(color).bold()));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// 名称超宽截断并加省略号（含中文按字符数处理）。
fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let cut: String = name.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// 12 轮色点：已测轮按当轮分档取色，未测轮暗灰小点。
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

/// 某组的目标数。
fn group_len(group: GroupId) -> usize {
    TARGETS.iter().filter(|t| t.group == group).count()
}
