//! IP 查询页（首页）：出口 IP 三卡、6 目标连通小卡与分流出口汇总。
//!
//! - 出口卡：国内双源（主/备，IP 不同才分卡）+ Cloudflare 出口，
//!   带国旗、中文归属地、来源标注与 iprisk 属性徽章；隐藏 IP 开关打码；
//! - 连通小卡：6 目标各「预热 1 次 + 12 轮取中位数」，分档着色；
//! - 分流汇总：37 站出口去重（国旗 + 来源站点）。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::net::geoip;
use crate::state::{EgressCard, EgressPhase, HomeState, LatencyState};
use crate::theme::color::{THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_TEXT};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv};
use crate::ui::layout::card_rects;
use crate::ui::mask::display_ip;

use super::{latency_color, latency_display};

/// 出口卡区高度（标题 + IP + 归属地 + 来源 + 徽章）。
const EGRESS_ROW_HEIGHT: u16 = 7;
/// 连通小卡区高度（两行小卡 + 间隔）。
const LATENCY_ROW_HEIGHT: u16 = 7;

/// 渲染首页。
pub fn render(f: &mut Frame, area: Rect, home: &HomeState, hide_ip: bool) {
    let [egress_row, latency_row, split_area] = Layout::vertical([
        Constraint::Length(EGRESS_ROW_HEIGHT),
        Constraint::Length(LATENCY_ROW_HEIGHT),
        Constraint::Min(0),
    ])
    .areas(area);

    render_egress(f, egress_row, home, hide_ip);
    render_latency_grid(f, latency_row, home);
    render_split(f, split_area, home, hide_ip);
}

/// 出口卡行：Pending 时给占位卡，Ready 时按卡数均分。
fn render_egress(f: &mut Frame, area: Rect, home: &HomeState, hide_ip: bool) {
    match &home.egress {
        EgressPhase::Pending => {
            let block = card(icon::labeled(Icon::IpQuery, "我的出口"));
            f.render_widget(block, area);
        }
        EgressPhase::Ready(cards) => {
            for (rect, card_data) in card_rects(area, cards.len()).into_iter().zip(cards) {
                render_egress_card(f, rect, card_data, hide_ip);
            }
        }
    }
}

/// 单张出口卡：标题 + 国旗 IP + 归属地 + 来源 + 属性徽章。
fn render_egress_card(f: &mut Frame, area: Rect, data: &EgressCard, hide_ip: bool) {
    let block = card(data.label.clone());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans = vec![Span::styled(
        format!("{} {}", data.flag, display_ip(&data.ip, hide_ip)),
        Style::new().fg(THEME_TEXT).bold(),
    )];
    for badge_label in &data.badges {
        spans.push(Span::raw(" "));
        spans.push(badge(badge_label, THEME_SUCCESS));
    }
    let lines = vec![
        Line::from(spans),
        kv("归属地", &data.geo),
        kv("来源", &data.source),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// 6 目标连通小卡：3 列 × 2 行网格。
fn render_latency_grid(f: &mut Frame, area: Rect, home: &HomeState) {
    let block = card(icon::labeled(Icon::Gauge, "网络连通（中位数）"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let slots = card_rects(
        inner,
        home.latency
            .len()
            .max(crate::probe::HOME_LATENCY_TARGETS.len()),
    );
    for (rect, target_state) in slots.into_iter().zip(home.latency.iter()) {
        render_latency_slot(f, rect, target_state);
    }
}

/// 单个连通小卡：名称 + 分档延迟 + 12 轮色点。
fn render_latency_slot(f: &mut Frame, area: Rect, target_state: &LatencyState) {
    let (text, color) = latency_display(target_state);
    let lines = vec![
        Line::from(Span::styled(
            target_state.name.to_string(),
            Style::new().fg(THEME_TEXT),
        )),
        Line::from(Span::styled(text, Style::new().fg(color).bold())),
        round_dots(target_state),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// 12 轮色点：已测轮按当轮分档取色，未测轮暗灰小点。
fn round_dots(target_state: &LatencyState) -> Line<'static> {
    use crate::net::latency::RoundPlan;
    use crate::net::latency::tier as round_tier;

    let total = RoundPlan::HOME.rounds as usize;
    let spans = (0..total)
        .map(|i| match target_state.rounds.get(i) {
            Some(result) => Span::styled("●", Style::new().fg(latency_color(round_tier(*result)))),
            None => Span::styled("·", Style::new().fg(THEME_MUTED)),
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// 分流出口汇总卡。
fn render_split(f: &mut Frame, area: Rect, home: &HomeState, hide_ip: bool) {
    let title = if home.split_done {
        icon::labeled(
            Icon::Web,
            &format!("分流出口汇总（{} 站）", home.split.len()),
        )
    } else {
        icon::labeled(
            Icon::Web,
            &format!(
                "分流出口探测中 {}/{} 站",
                home.split_resolved(),
                home.split.len()
            ),
        )
    };
    let block = card(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for exit in &home.split_summary {
        let sites = exit.sites.join("、");
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{} {}",
                    geoip::flag_emoji(&exit.country_code),
                    display_ip(&exit.ip, hide_ip)
                ),
                Style::new().fg(THEME_TEXT).bold(),
            ),
            Span::styled(format!("　{sites}"), Style::new().fg(THEME_MUTED)),
        ]));
    }
    let failed = home.split.len() - home.split_resolved();
    if !home.split_done {
        lines.push(Line::styled(
            "正在逐站获取出口 IP，完成后自动汇总…".to_string(),
            Style::new().fg(THEME_MUTED),
        ));
    } else if home.split_summary.is_empty() {
        lines.push(Line::styled(
            "未获取到任何分流出口".to_string(),
            Style::new().fg(THEME_ERROR),
        ));
    } else if failed > 0 {
        lines.push(Line::styled(
            format!("另有 {failed} 站未获取到出口"),
            Style::new().fg(THEME_MUTED),
        ));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "准备中…".to_string(),
            Style::new().fg(THEME_MUTED),
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}
