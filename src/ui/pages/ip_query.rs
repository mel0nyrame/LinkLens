//! IP 查询页（首页）：出口 IP 三卡、6 目标连通小卡与分流出口汇总。
//!
//! - 出口卡：国内双源（主/备，IP 不同才分卡）+ Cloudflare 出口，
//!   带国旗、中文归属地、来源标注与 iprisk 属性徽章；隐藏 IP 开关打码；
//! - 连通小卡：6 目标各「预热 1 次 + 12 轮取中位数」，分档着色；
//! - 分流汇总：目标清单出口去重（国旗 + 来源站点）。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::net::geoip;
use crate::state::{EgressCard, EgressPhase, HomeState, LatencyState};
use crate::theme::color::{THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_TEXT};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv};
use crate::ui::layout::content_card_rects;
use crate::ui::mask::display_ip;
use crate::ui::scroll::{card_canvas, copy_viewport};

use super::{latency_color, latency_display};

/// 按正文高度排列出口、连通小卡和完整分流列表。
pub fn canvas(width: u16, home: &HomeState, hide_ip: bool) -> Buffer {
    let cards = match &home.egress {
        EgressPhase::Pending => vec![Paragraph::new("正在探测出口…").block(card("我的出口"))],
        EgressPhase::Ready(cards) if cards.is_empty() => vec![
            Paragraph::new(Line::styled(
                "未获取到出口 IP（探测失败或超时）",
                Style::new().fg(THEME_ERROR),
            ))
            .block(card("我的出口")),
        ],
        EgressPhase::Ready(cards) => cards
            .iter()
            .map(|c| render_egress_card(c, hide_ip))
            .collect(),
    };
    let egress = card_canvas(width, cards);
    let (_, grid_height) = content_card_rects(width.saturating_sub(4), &[3; 6]);
    let latency_height = grid_height.saturating_add(2);
    let split = render_split(home, hide_ip);
    let split_height = split
        .line_count(width.saturating_sub(4).max(1))
        .min(usize::from(u16::MAX)) as u16;
    let latency_y = egress.area.height.saturating_add(1);
    let split_y = latency_y.saturating_add(latency_height).saturating_add(1);
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, split_y.saturating_add(split_height)));
    copy_viewport(&egress, &mut buffer, egress.area, 0);
    render_latency_grid(
        &mut buffer,
        Rect::new(0, latency_y, width, latency_height),
        home,
    );
    split.render(Rect::new(0, split_y, width, split_height), &mut buffer);
    buffer
}

/// 单张出口卡：标题 + 国旗 IP + 归属地 + 来源 + 属性徽章。
fn render_egress_card(data: &EgressCard, hide_ip: bool) -> Paragraph<'static> {
    let block = card(data.label.clone());

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
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
}

/// 6 目标连通小卡：3 列 × 2 行网格。
fn render_latency_grid(buffer: &mut Buffer, area: Rect, home: &HomeState) {
    let block = card(icon::labeled(Icon::Gauge, "网络连通（中位数）"));
    let inner = block.inner(area);
    block.render(area, buffer);

    let (slots, _) = content_card_rects(inner.width, &[3; 6]);
    for (rect, target_state) in slots.into_iter().zip(home.latency.iter()) {
        render_latency_slot(
            buffer,
            Rect::new(inner.x + rect.x, inner.y + rect.y, rect.width, rect.height),
            target_state,
        );
    }
}

/// 单个连通小卡：名称 + 分档延迟 + 12 轮色点。
fn render_latency_slot(buffer: &mut Buffer, area: Rect, target_state: &LatencyState) {
    let (text, color) = latency_display(target_state);
    let lines = vec![
        Line::from(Span::styled(
            target_state.name.to_string(),
            Style::new().fg(THEME_TEXT),
        )),
        Line::from(Span::styled(text, Style::new().fg(color).bold())),
        round_dots(target_state),
    ];
    Paragraph::new(lines).render(area, buffer);
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
fn render_split(home: &HomeState, hide_ip: bool) -> Paragraph<'static> {
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
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
}
