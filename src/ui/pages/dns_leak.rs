//! DNS 泄漏页：随机子域走系统 resolver 的探测进度、解析器出口列表与三态判定。
//!
//! - 出口参照卡：1.1.1.1 trace 的 HTTP 出口（与解析器出口对照）；
//! - 结论卡：三态判定（泄漏 / 已加密未暴露 / 干净），语义色徽章；
//! - 解析器列表：逐项国旗 + IP + 中文归属地（geoip）；
//! - 底部提示：代理模式下 UDP 与系统 DNS 可能不通，TUN 模式结果才准确。

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::detect::leak::DnsVerdict;
use crate::state_leak::{DnsLeakState, DnsPhase};
use crate::theme::color::{THEME_ACCENT, THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_TEXT};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv};
use crate::ui::layout::card_grid_columns;
use crate::ui::mask::display_ip;

/// 渲染 DNS 泄漏页。
pub fn render(f: &mut Frame, area: Rect, dns: &DnsLeakState, hide_ip: bool) {
    let [top, list, hint] = Layout::vertical([
        Constraint::Length(TOP_HEIGHT),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    // 宽终端两卡并排，窄终端纵排
    if card_grid_columns(area.width) >= 3 {
        let [verdict_rect, egress_rect] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(top);
        render_verdict(f, verdict_rect, dns);
        render_egress(f, egress_rect, dns, hide_ip);
    } else {
        let [verdict_rect, egress_rect] = Layout::vertical([
            Constraint::Length(TOP_HEIGHT),
            Constraint::Length(TOP_HEIGHT),
        ])
        .areas(top);
        render_verdict(f, verdict_rect, dns);
        render_egress(f, egress_rect, dns, hide_ip);
    }

    render_resolvers(f, list, dns, hide_ip);
    f.render_widget(
        Paragraph::new(Line::styled(UDP_HINT, Style::new().fg(THEME_MUTED))),
        hint,
    );
}

/// 顶部两卡高度。
const TOP_HEIGHT: u16 = 5;
/// 提示文案：代理模式 UDP 不通、TUN 模式才准。
const UDP_HINT: &str = "提示：代理模式下 UDP 与系统 DNS 可能不通，结果仅供参考；TUN 模式下结果才准确。按 f 快速测试，d 深度测试";

/// 结论卡：三态判定的徽章与文案。
fn render_verdict(f: &mut Frame, area: Rect, dns: &DnsLeakState) {
    let block = card(icon::labeled(Icon::DnsLeak, "检测结论"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = match (dns.phase, &dns.verdict) {
        (DnsPhase::Idle, _) | (DnsPhase::Running, _) => {
            let progress = match dns.mode {
                Some(mode) => format!(
                    "{}：触发查询 {}/{} 轮",
                    mode.label(),
                    dns.rounds_done,
                    dns.rounds_total
                ),
                None => "等待发起".to_string(),
            };
            vec![
                Line::from(Span::styled(
                    if dns.phase == DnsPhase::Running {
                        format!("{progress}，探测中…")
                    } else {
                        progress
                    },
                    Style::new().fg(THEME_MUTED),
                )),
                Line::from(Span::styled(
                    "按 f 快速测试，d 深度测试",
                    Style::new().fg(THEME_MUTED),
                )),
            ]
        }
        (DnsPhase::Done, _) if dns.error.is_some() => vec![Line::from(Span::styled(
            dns.error.clone().unwrap_or_default(),
            Style::new().fg(THEME_ERROR),
        ))],
        (DnsPhase::Done, Some(verdict)) => {
            let (label, color, text) = verdict_display(*verdict);
            vec![
                Line::from(vec![
                    badge(label, color),
                    Span::styled(" ", Style::new()),
                    Span::styled(text.to_string(), Style::new().fg(color).bold()),
                ]),
                Line::styled(
                    "按 f 重新快速测试，d 深度测试".to_string(),
                    Style::new().fg(THEME_MUTED),
                ),
            ]
        }
        (DnsPhase::Done, None) => vec![Line::from(Span::styled(
            "探测已结束，但没有得到结论".to_string(),
            Style::new().fg(THEME_ERROR),
        ))],
    };
    f.render_widget(Paragraph::new(lines), inner);
}

/// 三态判定的展示三元组（徽章文案、语义色、结论文案，照上游接口报告判定文案）。
fn verdict_display(verdict: DnsVerdict) -> (&'static str, ratatui::style::Color, &'static str) {
    match verdict {
        DnsVerdict::Leak => (
            "泄漏",
            THEME_ERROR,
            "检测到 DNS 泄漏！暴露了中国大陆的 DNS 服务器",
        ),
        DnsVerdict::Encrypted => (
            "已加密",
            THEME_ACCENT,
            "未检测到 DNS 解析器出口 IP，你的 DNS 可能已加密（DoH/DoT）",
        ),
        DnsVerdict::Clean => ("干净", THEME_SUCCESS, "完美！未检测到 DNS 泄漏"),
    }
}

/// 出口参照卡：HTTP 出口 IP、归属地与国别。
fn render_egress(f: &mut Frame, area: Rect, dns: &DnsLeakState, hide_ip: bool) {
    let block = card(icon::labeled(Icon::Earth, "当前出口（HTTP）"));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = match &dns.egress_ip {
        Some(ip) => vec![
            Line::from(Span::styled(
                format!("{} {}", dns.egress_flag, display_ip(ip, hide_ip)),
                Style::new().fg(THEME_TEXT).bold(),
            )),
            kv("归属地", &dns.egress_geo),
            kv("国别", &dns.egress_country),
        ],
        None => vec![Line::styled(
            "出口获取中…".to_string(),
            Style::new().fg(THEME_MUTED),
        )],
    };
    f.render_widget(Paragraph::new(lines), inner);
}

/// 解析器出口列表卡：逐项国旗 + IP + 归属地。
fn render_resolvers(f: &mut Frame, area: Rect, dns: &DnsLeakState, hide_ip: bool) {
    let title = icon::labeled(
        Icon::Server,
        &format!("DNS 解析器出口（{} 个）", dns.resolvers.len()),
    );
    let block = card(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    match dns.phase {
        DnsPhase::Idle => lines.push(Line::styled(
            "尚未发起探测，按 f 或 d 开始".to_string(),
            Style::new().fg(THEME_MUTED),
        )),
        DnsPhase::Running => lines.push(Line::styled(
            "正在触发随机子域解析，稍后回读解析器列表…".to_string(),
            Style::new().fg(THEME_MUTED),
        )),
        DnsPhase::Done => {
            for resolver in &dns.resolvers {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} {}", resolver.flag, display_ip(&resolver.ip, hide_ip)),
                        Style::new().fg(THEME_TEXT).bold(),
                    ),
                    Span::styled(
                        format!("　{}", resolver.location),
                        Style::new().fg(THEME_MUTED),
                    ),
                ]));
            }
            if dns.resolvers.is_empty() && dns.error.is_none() {
                lines.push(Line::styled(
                    "未检测到 DNS 解析器出口 IP".to_string(),
                    Style::new().fg(THEME_ACCENT),
                ));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}
