//! Claude/GPT 检测页共用框架：两页同构，由 `probe_ai::AiProfile` 参数化渲染。
//!
//! 布局：三出口 IP 卡行（国内出口复用首页票 02 结果、Cloudflare 出口、AI 出口）→
//! 信任分 / 出口属性 / 安全检测卡行 → 可用性 / 服务状态 / 检测历史卡行。
//! 命中受限地区时信任分强制 0 + 红色「不可访问」且不显示时延。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::detect::ai::{self, AvailTier, StatusLevel, TrustTier};
use crate::history;
use crate::net::geoip;
use crate::probe_ai::AiProfile;
use crate::state::{EgressCard, EgressPhase, HomeState};
use crate::state_ai::{AiOutcome, AiPageState, AiPhase};
use crate::theme::color::{
    THEME_ACCENT, THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_SUCCESS_SOFT, THEME_TEXT,
    THEME_WARNING,
};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv, kv_styled, trust_bar};
use crate::ui::layout::card_rects;
use crate::ui::mask::display_ip;

/// 信任分条刻度宽度（列）。
const BAR_WIDTH: u16 = 24;

/// 渲染一个 AI 检测页。
pub fn render(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
    home: &HomeState,
    hide_ip: bool,
) {
    let [egress_row, detail_row, status_row] = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Min(0),
    ])
    .areas(area);

    render_egress_row(f, egress_row, profile, page_state, home, hide_ip);
    render_detail_row(f, detail_row, profile, page_state);
    render_status_row(f, status_row, profile, page_state, hide_ip);
}

// ---------- 第一行：三出口 IP 卡 ----------

/// 从首页结果拆出（国内出口卡, Cloudflare 出口卡）。
fn split_home_cards(home: &HomeState) -> (Option<&EgressCard>, Option<&EgressCard>) {
    match &home.egress {
        EgressPhase::Ready(cards) => {
            let cf = cards.iter().find(|c| c.label == "Cloudflare 出口");
            let cn = cards.iter().find(|c| c.label != "Cloudflare 出口");
            (cn, cf)
        }
        EgressPhase::Pending => (None, None),
    }
}

fn render_egress_row(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
    home: &HomeState,
    hide_ip: bool,
) {
    let (cn, cf) = split_home_cards(home);
    let rects = card_rects(area, 3);
    render_home_slot(f, rects[0], "国内出口", cn, hide_ip);
    render_home_slot(f, rects[1], "Cloudflare 出口", cf, hide_ip);
    render_ai_exit_card(f, rects[2], profile, page_state, hide_ip);
}

/// 首页复用卡槽：无数据时给「等待首页探测」占位。
fn render_home_slot(
    f: &mut Frame,
    area: Rect,
    label: &str,
    data: Option<&EgressCard>,
    hide_ip: bool,
) {
    let Some(data) = data else {
        let block = card(label.to_string());
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(Line::styled("等待首页探测…", Style::new().fg(THEME_MUTED))),
            inner,
        );
        return;
    };
    let block = card(data.label.clone());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(Span::styled(
            format!("{} {}", data.flag, display_ip(&data.ip, hide_ip)),
            Style::new().fg(THEME_TEXT).bold(),
        )),
        kv("归属地", &data.geo),
        kv("来源", &data.source),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// AI 出口卡：trace 解析的「平台看到的你的 IP」。
fn render_ai_exit_card(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
    hide_ip: bool,
) {
    let title = format!("{} 出口", profile.brand);
    let block = card(title.clone());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        f.render_widget(
            Paragraph::new(Line::styled(
                format!("正在探测 {title}…"),
                Style::new().fg(THEME_MUTED),
            )),
            inner,
        );
        return;
    };

    let country_code = exit_country_code(outcome);
    let mut spans = Vec::new();
    if let Some(ip) = outcome.exit.as_ref().and_then(|t| t.ip.as_deref()) {
        spans.push(Span::styled(
            format!("{} ", geoip::flag_emoji(&country_code)),
            Style::new(),
        ));
        spans.push(Span::styled(
            display_ip(ip, hide_ip),
            Style::new().fg(THEME_TEXT).bold(),
        ));
    } else {
        spans.push(Span::styled(
            "获取失败".to_string(),
            Style::new().fg(THEME_ERROR),
        ));
    }
    if outcome.restricted {
        spans.push(Span::raw(" "));
        spans.push(badge("不可访问", THEME_ERROR));
    }
    let mut lines = vec![Line::from(spans)];
    lines.push(kv("归属地", &exit_geo_text(outcome)));
    lines.push(kv("来源", &format!("{} trace", profile.trace_host)));
    f.render_widget(Paragraph::new(lines), inner);
}

/// 出口国别码（geoip 优先，trace loc 兜底；与受限判定同源）。
fn exit_country_code(outcome: &AiOutcome) -> String {
    ai::resolve_exit_country(
        outcome.geo.as_ref(),
        outcome.exit.as_ref().and_then(|t| t.loc.as_deref()),
    )
    .map(|c| c.code)
    .unwrap_or_default()
}

/// AI 出口卡的归属地文本；trace 兜底时只给中文国名（表外空串由界面兜住）。
fn exit_geo_text(outcome: &AiOutcome) -> String {
    let country_code = exit_country_code(outcome);
    if country_code.is_empty() {
        return "未知".to_string();
    }
    if outcome.geo_from_trace {
        let name = ai::fallback_location_text(&country_code);
        if name.is_empty() {
            return country_code.to_uppercase();
        }
        return name;
    }
    outcome
        .geo
        .as_ref()
        .map(crate::net::cc::chinese_location)
        .unwrap_or_else(|| "未知".to_string())
}

// ---------- 第二行：信任分 / 出口属性 / 安全检测 ----------

fn render_detail_row(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
) {
    let rects = card_rects(area, 3);
    render_trust_card(f, rects[0], profile, page_state);
    render_property_card(f, rects[1], page_state);
    render_security_card(f, rects[2], page_state);
}

/// 信任分卡：分值 + 五档徽章 + 渐变刻度条；受限强制 0 并红色警示。
fn render_trust_card(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
) {
    let block = card(icon::labeled(Icon::Shield, "信任分"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        render_pending_hint(f, inner);
        return;
    };

    let restricted = outcome.restricted;
    let score = ai::effective_score(
        outcome.risk.as_ref().and_then(|r| r.trust_score),
        restricted,
    );

    let mut lines: Vec<Line<'static>> = Vec::new();
    match score {
        Some(score) => {
            let tier = ai::trust_tier(score);
            let mut head = vec![
                Span::styled(
                    format!("{score:>3} "),
                    Style::new().fg(trust_tier_color(tier)).bold(),
                ),
                Span::styled("/ 100 ", Style::new().fg(THEME_MUTED)),
                badge(tier.label(), trust_tier_color(tier)),
            ];
            if restricted {
                head.push(Span::raw(" "));
                head.push(badge("不可访问", THEME_ERROR));
            }
            lines.push(Line::from(head));
            lines.push(trust_bar(
                score,
                BAR_WIDTH.min(inner.width.saturating_sub(2)),
            ));
        }
        None => {
            lines.push(Line::styled(
                "风险库数据未获取".to_string(),
                Style::new().fg(THEME_MUTED),
            ));
        }
    }
    if restricted {
        lines.push(Line::styled(
            profile.restricted_hint.to_string(),
            Style::new().fg(THEME_ERROR).bold(),
        ));
        lines.push(Line::styled(
            "出口命中受限地区，信任分强制按 0 处理".to_string(),
            Style::new().fg(THEME_MUTED),
        ));
    }
    // 陷阱如实标注：iprisk 按 CIDR 段聚合，响应 IP 可能是段代表而非请求 IP
    let risk_ip = outcome.risk.as_ref().map(|r| r.ip.as_str());
    let requested_ip = outcome.exit.as_ref().and_then(|t| t.ip.as_deref());
    if let (Some(risk_ip), Some(requested_ip)) = (risk_ip, requested_ip)
        && !risk_ip.is_empty()
        && risk_ip != requested_ip
    {
        lines.push(kv_styled(
            "风险库 IP",
            &format!("{risk_ip}（段代表，非本次请求 IP）"),
            Style::new().fg(THEME_WARNING),
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// 出口属性卡：地区/城市/IP 属性/ASN/运营商。
fn render_property_card(f: &mut Frame, area: Rect, page_state: &AiPageState) {
    let block = card(icon::labeled(Icon::Server, "出口属性"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        render_pending_hint(f, inner);
        return;
    };
    let Some(risk) = outcome.risk.as_ref() else {
        f.render_widget(
            Paragraph::new(Line::styled(
                "出口属性未获取".to_string(),
                Style::new().fg(THEME_MUTED),
            )),
            inner,
        );
        return;
    };

    let unknown = "未知";
    let mut lines = vec![
        kv("地区", &non_empty(&risk.region, unknown)),
        kv("城市", &non_empty(&risk.city, unknown)),
    ];
    match risk.property_badge() {
        Some(property) => lines.push(Line::from(vec![
            Span::styled("IP 属性：", Style::new().fg(THEME_MUTED)),
            badge(property, THEME_SUCCESS),
        ])),
        None => lines.push(kv("IP 属性", unknown)),
    }
    lines.push(kv(
        "ASN",
        &risk
            .asn
            .map_or_else(|| unknown.to_string(), |asn| format!("AS{asn}")),
    ));
    lines.push(kv(
        "运营商",
        &non_empty(
            if risk.as_organization.is_empty() {
                &risk.company_name
            } else {
                &risk.as_organization
            },
            unknown,
        ),
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

/// 安全检测卡：VPN/代理/Tor/机器人/滥用记录逐项徽章。
fn render_security_card(f: &mut Frame, area: Rect, page_state: &AiPageState) {
    let block = card(icon::labeled(Icon::Lock, "安全检测"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        render_pending_hint(f, inner);
        return;
    };
    let Some(risk) = outcome.risk.as_ref() else {
        f.render_widget(
            Paragraph::new(Line::styled(
                "安全信号未获取".to_string(),
                Style::new().fg(THEME_MUTED),
            )),
            inner,
        );
        return;
    };

    let lines = ai::security_items(risk)
        .into_iter()
        .map(|(name, hit)| {
            Line::from(vec![
                Span::styled(format!("{name}："), Style::new().fg(THEME_MUTED)),
                if hit {
                    badge("命中", THEME_ERROR)
                } else {
                    badge("正常", THEME_SUCCESS)
                },
            ])
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), inner);
}

// ---------- 第三行：可用性 / 服务状态 / 检测历史 ----------

fn render_status_row(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
    hide_ip: bool,
) {
    let rects = card_rects(area, 3);
    render_availability_card(f, rects[0], page_state);
    render_service_card(f, rects[1], profile, page_state);
    render_history_card(f, rects[2], page_state, hide_ip);
}

/// 可用性卡：逐目标时延 + 总体分档；受限地区一律「不可访问」且不显示时延。
fn render_availability_card(f: &mut Frame, area: Rect, page_state: &AiPageState) {
    let block = card(icon::labeled(Icon::Gauge, "可用性"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        render_pending_hint(f, inner);
        return;
    };

    if outcome.restricted {
        let lines = vec![
            Line::styled("不可访问".to_string(), Style::new().fg(THEME_ERROR).bold()),
            Line::styled(
                "出口命中受限地区，不显示时延".to_string(),
                Style::new().fg(THEME_MUTED),
            ),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let mut lines: Vec<Line<'static>> = outcome
        .availability
        .iter()
        .map(|(name, latency)| {
            let tier = ai::availability_tier(*latency);
            let text = latency.map_or_else(
                || tier.label().to_string(),
                |ms| format!("{ms}ms（{}）", tier.label()),
            );
            Line::from(vec![
                Span::styled(format!("{name}："), Style::new().fg(THEME_MUTED)),
                Span::styled(text, Style::new().fg(avail_tier_color(tier))),
            ])
        })
        .collect();
    let best = ai::best_latency(
        &outcome
            .availability
            .iter()
            .map(|(_, latency)| *latency)
            .collect::<Vec<_>>(),
    );
    let tier = ai::availability_tier(best);
    lines.push(Line::from(vec![
        Span::styled("总体：", Style::new().fg(THEME_MUTED)),
        Span::styled(tier.label(), Style::new().fg(avail_tier_color(tier)).bold()),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

/// 服务状态卡：status.json 的中文结论与组件明细。
fn render_service_card(
    f: &mut Frame,
    area: Rect,
    profile: &'static AiProfile,
    page_state: &AiPageState,
) {
    let block = card(icon::labeled(
        Icon::Connection,
        &format!("{brand}服务状态", brand = profile.brand),
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let AiPhase::Done(outcome) = &page_state.phase else {
        render_pending_hint(f, inner);
        return;
    };
    let Some(status) = outcome.status.as_ref() else {
        f.render_widget(
            Paragraph::new(Line::styled(
                "服务状态获取失败".to_string(),
                Style::new().fg(THEME_MUTED),
            )),
            inner,
        );
        return;
    };

    let level = ai::status_level(&status.overall_indicator);
    let color = status_level_color(level);
    let mut lines = vec![Line::from(Span::styled(
        status.overall.clone(),
        Style::new().fg(color).bold(),
    ))];
    for component in &status.components {
        let text = if component.status_cn.is_empty() {
            component.status.clone()
        } else {
            component.status_cn.clone()
        };
        lines.push(kv(&component.name, &text));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// 检测历史卡：跨重启持久化，最新在前；按 r 一键重查当前出口。
fn render_history_card(f: &mut Frame, area: Rect, page_state: &AiPageState, hide_ip: bool) {
    let block = card(icon::labeled(
        Icon::Clock,
        &format!("检测历史（{} 条）", page_state.history.len()),
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for entry in &page_state.history {
        let mut spans = vec![Span::styled(
            display_ip(&entry.ip, hide_ip),
            Style::new().fg(THEME_TEXT).bold(),
        )];
        if entry.restricted {
            spans.push(Span::raw(" "));
            spans.push(badge("受限", THEME_ERROR));
        } else {
            spans.push(Span::styled(
                format!(" {}分", entry.trust_score),
                Style::new().fg(THEME_MUTED),
            ));
        }
        spans.push(Span::styled(
            format!("　{}", history::format_recorded_at(entry.recorded_at_ms)),
            Style::new().fg(THEME_MUTED),
        ));
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "暂无历史".to_string(),
            Style::new().fg(THEME_MUTED),
        ));
    }
    lines.push(Line::styled(
        icon::labeled(Icon::Refresh, "按 r 重查当前出口"),
        Style::new().fg(THEME_ACCENT),
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

// ---------- 档位取色与小组件 ----------

/// 信任分档位取色：绿 / 浅绿 / 黄 / 红。
fn trust_tier_color(tier: TrustTier) -> ratatui::style::Color {
    match tier {
        TrustTier::ExtremelyPure | TrustTier::Pure => THEME_SUCCESS,
        TrustTier::Good => THEME_SUCCESS_SOFT,
        TrustTier::Neutral => THEME_WARNING,
        TrustTier::Suspicious => THEME_ERROR,
    }
}

/// 可用性档位取色：绿 / 浅绿 / 黄 / 红。
fn avail_tier_color(tier: AvailTier) -> ratatui::style::Color {
    match tier {
        AvailTier::Normal => THEME_SUCCESS,
        AvailTier::Good => THEME_SUCCESS_SOFT,
        AvailTier::Slow => THEME_WARNING,
        AvailTier::Unreachable => THEME_ERROR,
    }
}

/// 服务状态取色：正常绿 / 维护青 / 轻微黄 / 重大与严重红 / 未知暗灰。
fn status_level_color(level: Option<StatusLevel>) -> ratatui::style::Color {
    match level {
        Some(StatusLevel::AllNormal) => THEME_SUCCESS,
        Some(StatusLevel::Maintenance) => THEME_ACCENT,
        Some(StatusLevel::Minor) => THEME_WARNING,
        Some(StatusLevel::Major | StatusLevel::Critical) => THEME_ERROR,
        None => THEME_MUTED,
    }
}

fn render_pending_hint(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new(Line::styled("探测中…", Style::new().fg(THEME_MUTED))),
        area,
    );
}

fn non_empty(text: &str, fallback: &str) -> String {
    if text.trim().is_empty() {
        fallback.to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::split_home_cards;
    use crate::state::{EgressCard, EgressPhase, HomeState};

    fn card(label: &str) -> EgressCard {
        EgressCard {
            label: label.to_string(),
            ip: "1.2.3.4".into(),
            ..EgressCard::default()
        }
    }

    #[test]
    fn home_cards_split_cn_and_cloudflare() {
        let home = HomeState {
            egress: EgressPhase::Ready(vec![
                card("主出口"),
                card("备用出口"),
                card("Cloudflare 出口"),
            ]),
            ..HomeState::default()
        };
        let (cn, cf) = split_home_cards(&home);
        assert_eq!(cn.map(|c| c.label.as_str()), Some("主出口"));
        assert_eq!(cf.map(|c| c.label.as_str()), Some("Cloudflare 出口"));
    }

    #[test]
    fn pending_home_has_no_slots() {
        let home = HomeState::default();
        let (cn, cf) = split_home_cards(&home);
        assert!(cn.is_none() && cf.is_none());
    }
}
