//! WebRTC 泄漏页：STUN 采集进度、公网 UDP 候选列表与泄漏判定。
//!
//! - 出口参照卡：1.1.1.1 trace 的 HTTP 出口（与 UDP 公网地址对照）；
//! - 结论卡：三态判定（可能泄漏 / 未暴露 / 一致），语义色徽章；
//! - 候选列表：逐项类型标注（公网 STUN / 中继 / 本地）+ 国旗 + IP；
//! - 底部提示：代理模式下 UDP 通常不通，TUN 模式结果才准确；按 r 重新探测。

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::detect::leak::{CandidateKind, WebrtcVerdict};
use crate::state_leak::{WebrtcPhase, WebrtcState};
use crate::theme::color::{
    THEME_ACCENT, THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_TEXT, THEME_WARNING,
};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv};
use crate::ui::mask::display_ip;
use crate::ui::scroll::card_canvas;

/// 提示文案：代理模式 UDP 不通、TUN 模式才准。
const UDP_HINT: &str = "提示：代理模式下 UDP 通常不通，采不到公网地址不代表没泄漏；TUN 模式下结果才准确。按 r 重新探测";

/// 按正文高度生成完整页面，供统一视口滚动。
pub fn canvas(width: u16, webrtc: &WebrtcState, hide_ip: bool) -> Buffer {
    card_canvas(
        width,
        vec![
            render_verdict(webrtc),
            render_egress(webrtc, hide_ip),
            render_candidates(webrtc, hide_ip),
            Paragraph::new(Line::styled(UDP_HINT, Style::new().fg(THEME_MUTED)))
                .block(card("链路提示")),
        ],
    )
}

/// 结论卡：三态判定的徽章与文案。
fn render_verdict(webrtc: &WebrtcState) -> Paragraph<'static> {
    let block = card(icon::labeled(Icon::WebRtc, "检测结论"));

    let lines = match (webrtc.phase, &webrtc.verdict) {
        (WebrtcPhase::Idle, _) => vec![Line::from(Span::styled(
            "进入本页即自动探测，稍候…".to_string(),
            Style::new().fg(THEME_MUTED),
        ))],
        (WebrtcPhase::Running, _) => vec![Line::from(Span::styled(
            format!(
                "正在向 STUN 服务器查询公网地址（{}/{}）…",
                webrtc.queries_done, webrtc.queries_total
            ),
            Style::new().fg(THEME_MUTED),
        ))],
        (WebrtcPhase::Done, _) if webrtc.error.is_some() => vec![Line::from(Span::styled(
            webrtc.error.clone().unwrap_or_default(),
            Style::new().fg(THEME_ERROR),
        ))],
        (WebrtcPhase::Done, Some(verdict)) => {
            let (label, color, text) = verdict_display(*verdict);
            vec![
                Line::from(vec![
                    badge(label, color),
                    Span::styled(" ", Style::new()),
                    Span::styled(text.to_string(), Style::new().fg(color).bold()),
                ]),
                Line::styled("按 r 重新探测".to_string(), Style::new().fg(THEME_MUTED)),
            ]
        }
        (WebrtcPhase::Done, None) => vec![Line::from(Span::styled(
            "探测已结束，但没有得到结论".to_string(),
            Style::new().fg(THEME_ERROR),
        ))],
    };
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
}

/// 三态判定的展示三元组（徽章文案、语义色、结论文案，照上游接口报告判定文案）。
fn verdict_display(verdict: WebrtcVerdict) -> (&'static str, Color, &'static str) {
    match verdict {
        WebrtcVerdict::MaybeLeak => (
            "可能泄漏",
            THEME_ERROR,
            "公网 UDP 地址与 HTTP 出口不一致，WebRTC 可能暴露真实 IP",
        ),
        WebrtcVerdict::NoPublic => (
            "未暴露",
            THEME_ACCENT,
            "未发现公网 UDP 地址，WebRTC 已禁用或未暴露",
        ),
        WebrtcVerdict::Clean => (
            "一致",
            THEME_SUCCESS,
            "公网 UDP 地址与 HTTP 出口一致，未发现泄漏",
        ),
    }
}

/// 出口参照卡：HTTP 出口 IP 与归属地。
fn render_egress(webrtc: &WebrtcState, hide_ip: bool) -> Paragraph<'static> {
    let block = card(icon::labeled(Icon::Earth, "当前出口（HTTP）"));

    let lines = match &webrtc.egress_ip {
        Some(ip) => vec![
            Line::from(Span::styled(
                format!("{} {}", webrtc.egress_flag, display_ip(ip, hide_ip)),
                Style::new().fg(THEME_TEXT).bold(),
            )),
            kv("归属地", &webrtc.egress_geo),
        ],
        None => vec![Line::styled(
            match webrtc.phase {
                WebrtcPhase::Idle => "尚未开始出口探测",
                WebrtcPhase::Running => "出口获取中…",
                WebrtcPhase::Done => "出口获取失败（探测失败或超时）",
            }
            .to_string(),
            Style::new().fg(if webrtc.phase == WebrtcPhase::Done {
                THEME_ERROR
            } else {
                THEME_MUTED
            }),
        )],
    };
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
}

/// 候选类型徽章色：公网 STUN 黄（暴露信号）、中继青、本地暗灰。
fn kind_color(kind: CandidateKind) -> Color {
    match kind {
        CandidateKind::PublicStun => THEME_WARNING,
        CandidateKind::Relay => THEME_ACCENT,
        CandidateKind::Local => THEME_MUTED,
    }
}

/// 候选列表卡：逐项类型徽章 + 国旗 + IP。
fn render_candidates(webrtc: &WebrtcState, hide_ip: bool) -> Paragraph<'static> {
    let title = icon::labeled(
        Icon::Connection,
        &format!("ICE 候选（公网 {} 个）", webrtc.candidates.len()),
    );
    let block = card(title);

    let mut lines: Vec<Line<'static>> = Vec::new();
    match webrtc.phase {
        WebrtcPhase::Idle | WebrtcPhase::Running => lines.push(Line::styled(
            format!(
                "正在采集 STUN 公网地址（{}/{}）…",
                webrtc.queries_done, webrtc.queries_total
            ),
            Style::new().fg(THEME_MUTED),
        )),
        WebrtcPhase::Done => {
            for candidate in &webrtc.candidates {
                lines.push(Line::from(vec![
                    badge(candidate.kind.label(), kind_color(candidate.kind)),
                    Span::styled(
                        format!(" {}", display_ip(&candidate.ip.to_string(), hide_ip)),
                        Style::new().fg(THEME_TEXT).bold(),
                    ),
                ]));
            }
            if webrtc.candidates.is_empty() && webrtc.error.is_none() {
                lines.push(Line::styled(
                    "未发现公网 UDP 地址（UDP 可能被代理拦截）".to_string(),
                    Style::new().fg(THEME_ACCENT),
                ));
            }
        }
    }
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
}
