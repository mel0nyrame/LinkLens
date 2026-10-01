//! IP 评分：固定输入栏与按内容高度展开的可滚动卡片。
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use serde_json::Value;

use super::trust_tier_color;
use crate::detect::ai::trust_tier;
use crate::detect::scene::{self, IpOrigin, Scene};
use crate::net::geoip::flag_emoji;
use crate::net::ip_score::{BgpNode, Lookup};
use crate::state_score::{ScorePhase, ScoreState, Section};
use crate::theme::color::{THEME_ACCENT, THEME_ERROR, THEME_MUTED, THEME_SUCCESS, THEME_WARNING};
use crate::theme::icon::{self, Icon};
use crate::theme::widget::{badge, card, kv, trust_bar};
use crate::ui::layout::measured_card_rects;
use crate::ui::mask::display_ip_text;
use crate::ui::scroll::copy_viewport;

struct ScoreCard {
    title: &'static str,
    lines: Vec<Line<'static>>,
}

impl ScoreCard {
    fn new(title: &'static str, lines: Vec<Line<'static>>) -> Self {
        Self { title, lines }
    }
    fn paragraph(&self) -> Paragraph<'_> {
        Paragraph::new(self.lines.clone()).wrap(Wrap { trim: false })
    }
    fn title(&self) -> String {
        let icon = match self.title {
            "信任分" => Icon::Gauge,
            "AI 判词与原生/广播" => Icon::Fingerprint,
            "IP 属性" | "ASN / 运营商" | "同 ASN 公司" | "机房邻居" => Icon::Server,
            "技术指标" => Icon::Network,
            "安全信号" | "DNSBL 黑名单 · 12 家" => Icon::Shield,
            "位置历史" | "ASN 历史" | "公司历史" => Icon::Clock,
            "中文归属地" | "多源定位 · g1 > g7 > g3 > g2" => Icon::Earth,
            "反查域名" => Icon::Web,
            "BGP 路由拓扑" => Icon::Network,
            "Radar 人机流量" | "C 段热度 · 近 14 天" => Icon::Radar,
            "非公网地址" => Icon::Warning,
            _ => Icon::IpScore,
        };
        icon::labeled(icon, self.title)
    }
}

fn content_area(area: Rect, state: &ScoreState, hide_ip: bool) -> Rect {
    let header_height = header(state, hide_ip)
        .line_count(area.width.saturating_sub(4).max(1))
        .saturating_add(2)
        .min(usize::from(area.height)) as u16;
    Rect::new(
        area.x,
        area.y + header_height,
        area.width,
        area.height - header_height,
    )
}

fn geometry(width: u16, cards: &[ScoreCard]) -> (Vec<Rect>, u16) {
    measured_card_rects(width, cards.len(), |i, card_width| {
        cards[i]
            .paragraph()
            .line_count(card_width.saturating_sub(4).max(1))
            .saturating_add(2)
            .min(usize::from(u16::MAX)) as u16
    })
}

pub fn max_scroll(area: Rect, state: &ScoreState, hide_ip: bool) -> u16 {
    let viewport = content_area(area, state, hide_ip);
    let (_, height) = geometry(viewport.width, &cards(state, hide_ip));
    height.saturating_sub(viewport.height)
}

pub fn render(f: &mut Frame, area: Rect, state: &ScoreState, hide_ip: bool) {
    if area.is_empty() {
        return;
    }
    let viewport = content_area(area, state, hide_ip);
    f.render_widget(
        header(state, hide_ip).block(card(icon::labeled(
            Icon::IpScore,
            "IP 评分 · 任意 IPv4 / IPv6",
        ))),
        Rect::new(area.x, area.y, area.width, viewport.y - area.y),
    );
    if viewport.is_empty() {
        return;
    }
    let cards = cards(state, hide_ip);
    let (rects, height) = geometry(viewport.width, &cards);
    let mut buffer = Buffer::empty(Rect::new(0, 0, viewport.width, height));
    for (c, rect) in cards.iter().zip(rects) {
        c.paragraph()
            .block(card(c.title()))
            .render(rect, &mut buffer);
    }
    copy_viewport(
        &buffer,
        f.buffer_mut(),
        viewport,
        state.scroll.min(height.saturating_sub(viewport.height)),
    );
}

fn header(state: &ScoreState, hide_ip: bool) -> Paragraph<'static> {
    let input = if hide_ip {
        "输入已隐藏".into()
    } else if state.input.is_empty() {
        "自动识别当前出口，或按 / 输入 IPv4 / IPv6".into()
    } else {
        state.input.clone()
    };
    let recent = state
        .recent
        .get(state.recent_index)
        .map(|ip| format!("{}/{} {}", state.recent_index + 1, state.recent.len(), ip))
        .unwrap_or_else(|| "暂无记录".into());
    let mut header = vec![
        kv(
            if state.editing {
                "正在输入"
            } else {
                "查询 IP"
            },
            &input,
        ),
        kv("最近查询 [ / ]", &recent),
        styled(
            state.phase.label(),
            if matches!(state.phase, ScorePhase::Failed(_)) {
                THEME_ERROR
            } else {
                THEME_ACCENT
            },
        ),
        styled(
            "/ 编辑 · Enter 查询/重查选中记录 · Esc 取消编辑 · r 重查 · ↑↓/PgUp/PgDn/Home/End 滚动",
            THEME_MUTED,
        ),
    ];
    if let Some(error) = state.input_error {
        header.push(styled(error, THEME_ERROR));
    }
    mask_lines(&mut header, hide_ip);
    Paragraph::new(header).wrap(Wrap { trim: false })
}

fn styled(text: impl Into<String>, color: Color) -> Line<'static> {
    Line::styled(text.into(), Style::new().fg(color))
}

fn mask_lines(lines: &mut [Line<'static>], hide_ip: bool) {
    if hide_ip {
        for line in lines {
            for span in &mut line.spans {
                span.content = display_ip_text(&span.content, true).into();
            }
        }
    }
}

fn status<T>(section: &Section<T>) -> Vec<Line<'static>> {
    match section {
        Section::Waiting => vec![styled("等待主资料…", THEME_MUTED)],
        Section::Pending { attempt, limit } => vec![styled(
            format!("正在补全… 第 {attempt}/{limit} 次"),
            THEME_WARNING,
        )],
        Section::Failed(error) => vec![styled(error.0.clone(), THEME_ERROR)],
        Section::Unsupported => vec![styled("此地址暂不支持", THEME_MUTED)],
        Section::Ready(_) => Vec::new(),
    }
}

fn section_card<T>(
    title: &'static str,
    section: &Section<T>,
    ready: impl FnOnce(&T) -> Vec<Line<'static>>,
) -> ScoreCard {
    ScoreCard::new(
        title,
        match section {
            Section::Ready(data) => ready(data),
            _ => status(section),
        },
    )
}

fn cards(s: &ScoreState, hide_ip: bool) -> Vec<ScoreCard> {
    let mut out = Vec::new();
    if let Section::Ready(d) = &s.lookup {
        if d.is_bogon {
            let mut lines = vec![
                Line::from(badge("非公网地址 · 不可路由", THEME_ERROR)),
                kv("IP", &d.risk.ip),
                kv(
                    "原因",
                    if d.bogon_reason == "Loopback" {
                        "回环地址"
                    } else {
                        &d.bogon_reason
                    },
                ),
                kv("依据", &d.bogon_rfc),
            ];
            if let Some(ai) = &d.ai_verdict {
                lines.push(Line::raw(ai.reasoning.clone()));
            }
            if matches!(s.geo, Section::Failed(_)) {
                lines.extend(status(&s.geo));
            }
            mask_lines(&mut lines, hide_ip);
            return vec![ScoreCard::new("非公网地址", lines)];
        }
        deep_cards(&mut out, d);
    } else {
        out.push(ScoreCard::new("深度资料", status(&s.lookup)));
    }
    out.push(section_card("中文归属地", &s.geo, |g| {
        vec![kv(
            "归属地",
            &format!(
                "{} {}",
                flag_emoji(&g.country_code),
                crate::net::cc::chinese_location(g)
            ),
        )]
    }));
    let mut related = section_card("反查域名", &s.related, |r| {
        let mut lines = vec![kv("数量", &r.related_domains.len().to_string())];
        for d in &r.related_domains {
            lines.push(kv(&d.domain, &d.via));
        }
        if r.pending {
            lines.push(styled(
                "后台仍在扫描，已达到本轮轮询上限；按 r 重查",
                THEME_WARNING,
            ));
        }
        lines
    });
    if !matches!(s.related, Section::Ready(_))
        && let Section::Ready(d) = &s.lookup
    {
        for domain in &d.related_domains {
            related.lines.push(kv(&domain.domain, &domain.via));
        }
    }
    out.push(related);
    if !matches!(s.heat, Section::Unsupported) {
        out.push(section_card("C 段热度 · 近 14 天", &s.heat, |h| {
            let mut lines = vec![
                kv("网段", &format!("{}.0/24", h.base)),
                kv("热度", &h.mode),
                kv(
                    "段内位置",
                    &h.idx
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "未知".into()),
                ),
            ];
            if let Some(chg) = h.chg {
                lines.push(kv("变化", &format!("{chg:+}%")));
            }
            for (i, value) in h.vals.iter().enumerate() {
                lines.push(kv(
                    h.days.get(i).map(String::as_str).unwrap_or("观测日"),
                    &value.to_string(),
                ));
            }
            append_value(&mut lines, "峰值", &h.peak);
            lines
        }));
    }
    out.push(section_card("BGP 路由拓扑", &s.bgp, |b| {
        let mut lines = vec![kv("前缀", &b.prefix), kv("观测路径", &b.paths.to_string())];
        for (label, nodes) in [
            ("起源 ASN", &b.origins),
            ("上游", &b.upstreams),
            ("二级上游", &b.second),
        ] {
            lines.push(styled(label, THEME_ACCENT));
            if nodes.is_empty() {
                lines.push(styled("暂无记录", THEME_MUTED));
            }
            for node in nodes {
                lines.extend(bgp_node(node));
            }
        }
        append_records(&mut lines, "历史快照", &b.snapshots);
        lines
    }));
    if !matches!(s.dnsbl, Section::Unsupported) {
        out.push(section_card("DNSBL 黑名单 · 12 家", &s.dnsbl, |d| {
            let mut lines = Vec::new();
            if d.results.is_empty() {
                lines.push(styled("暂无检测结果", THEME_MUTED));
            }
            for r in &d.results {
                let verdict = r.verdict();
                lines.push(Line::from(vec![
                    Span::raw(format!("{} ", r.engine)),
                    badge(
                        verdict,
                        if verdict.starts_with("纯净") {
                            THEME_SUCCESS
                        } else if verdict == "黑名单" {
                            THEME_ERROR
                        } else {
                            THEME_WARNING
                        },
                    ),
                ]));
                lines.push(kv("检测区", &r.zone));
                lines.push(kv("类别", &r.category));
                lines.push(kv("状态", &r.status));
                if !r.codes.is_empty() {
                    lines.push(kv("返回码", &r.codes.join("、")));
                }
                if let Some(ms) = r.ms {
                    lines.push(kv("耗时", &format!("{ms} ms")));
                }
            }
            lines
        }));
    }
    let mut radar = status(&s.radar);
    let raw = if let Section::Ready(r) = &s.radar {
        radar.push(kv("统计区间", &r.range));
        radar.push(kv(
            "缓存",
            if r.stale {
                "已过期"
            } else if r.cached {
                "缓存结果"
            } else {
                "实时结果"
            },
        ));
        if let Some(h) = r.human {
            radar.push(kv("ASN 人类占比", &format!("{h:.2}%")));
        }
        if let Some(b) = r.bot {
            radar.push(kv("ASN 机器人占比", &format!("{b:.2}%")));
        }
        r.ok.then_some(r.human).flatten()
    } else {
        None
    };
    if let Section::Ready(d) = &s.lookup {
        let (human, estimated) = scene::human_percent(d, raw);
        radar.push(kv("本 IP 人类流量", &format!("{human:.2}%")));
        radar.push(kv("本 IP 机器人流量", &format!("{:.2}%", 100.0 - human)));
        radar.push(styled(
            if estimated {
                "估算值 · 按网络属性与风险信号修正"
            } else {
                "Radar 统计值 · 按本 IP 风险信号修正"
            },
            if estimated {
                THEME_WARNING
            } else {
                THEME_ACCENT
            },
        ));
    }
    out.push(ScoreCard::new("Radar 人机流量", radar));
    out.push(section_card("同 ASN 公司", &s.companies, |c| {
        let mut lines = vec![kv("总档案", &c.total_profiles.to_string())];
        for company in &c.companies {
            lines.push(kv(
                &company.name,
                &format!("{} · {} 份档案", company.kind, company.n),
            ));
        }
        if c.companies.is_empty() {
            lines.push(styled("暂无公司记录", THEME_MUTED));
        }
        if c.pending {
            lines.push(styled(
                "后台仍在扫描，已达到本轮轮询上限；按 r 重查",
                THEME_WARNING,
            ));
        }
        lines
    }));
    for c in &mut out {
        mask_lines(&mut c.lines, hide_ip);
    }
    out
}

fn deep_cards(out: &mut Vec<ScoreCard>, d: &Lookup) {
    let mut verdict = Vec::new();
    if let Some(ai) = &d.ai_verdict {
        verdict.push(styled(ai.label.clone(), THEME_ACCENT));
        if let Some(confidence) = ai.confidence {
            verdict.push(kv("置信度", &format!("{confidence:.0}%")));
        }
        verdict.push(Line::raw(ai.reasoning.clone()));
    } else {
        verdict.push(styled("暂无 AI 判词", THEME_MUTED));
    }
    verdict.push(kv(
        "归属国",
        &format!(
            "{} {} ({})",
            flag_emoji(&d.risk.country_code),
            d.country,
            d.risk.country_code
        ),
    ));
    verdict.push(kv(
        "注册国",
        &format!(
            "{} {} ({})",
            flag_emoji(&d.registered_country_code),
            d.registered_country,
            d.registered_country_code
        ),
    ));
    match scene::ip_origin(&d.risk.country_code, &d.registered_country_code) {
        IpOrigin::Native => verdict.push(Line::from(badge("原生 IP", THEME_SUCCESS))),
        IpOrigin::Broadcast => verdict.push(Line::from(badge("广播 IP", THEME_WARNING))),
        IpOrigin::Unknown => verdict.push(styled("国别资料不足，无法判定原生/广播", THEME_MUTED)),
    }
    out.push(ScoreCard::new("AI 判词与原生/广播", verdict));
    let mut trust = Vec::new();
    if let Some(score) = d.risk.trust_score {
        trust.push(kv("信任分", &format!("{score}/100")));
        let tier = trust_tier(score);
        let color = trust_tier_color(tier);
        trust.push(Line::from(badge(tier.label(), color)));
        trust.push(trust_bar(score, 20));
    } else {
        trust.push(styled("暂无信任分", THEME_MUTED));
    }
    trust.push(kv("段代表 IP", &d.risk.ip));
    trust.push(kv("CIDR", &d.risk.cidr));
    trust.push(styled(
        "信任分按网段聚合，段代表 IP 可能与查询 IP 不同",
        THEME_MUTED,
    ));
    out.push(ScoreCard::new("信任分", trust));
    let mut attributes = vec![
        kv("资料来源", &d.src),
        kv("属性", d.risk.property_badge().unwrap_or("未知")),
        kv("公司", &d.risk.company_name),
        kv("公司类型", &d.risk.company_type),
        kv("地区", &d.risk.region),
        kv("城市", &d.risk.city),
        kv("机房", &d.datacenter_name),
    ];
    let mut asn = vec![
        kv(
            "ASN",
            &d.risk
                .asn
                .map(|n| format!("AS{n}"))
                .unwrap_or_else(|| "未知".into()),
        ),
        kv("ASN 组织", &d.risk.as_organization),
        kv("ASN 名称", &d.asname),
        kv("ASN 类型", &d.asn_kind),
        kv("ASN 带宽", &d.asn_tbps),
        kv("ASN 分配日", &d.asn_allocated),
        kv("ISP", &d.isp),
    ];
    if let Some(n) = d.asn_ipv4_count {
        asn.push(kv("ASN IPv4 数量", &n.to_string()));
    }
    out.push(ScoreCard::new("ASN / 运营商", asn));
    let mut technical = vec![kv("反向 DNS", &d.rdns), kv("RPKI", &d.rpki_status)];
    append_value(&mut technical, "地址范围", &d.range);
    out.push(ScoreCard::new("技术指标", technical));
    for (name, value) in [
        ("机房", d.risk.is_datacenter),
        ("家庭宽带", d.risk.is_residential),
        ("移动网络", Some(d.is_mobile)),
        ("公共服务", Some(d.is_public_service)),
        ("保留地址", Some(d.is_bogon)),
    ] {
        attributes.push(signal(name, value, THEME_ACCENT));
    }
    append_value(&mut attributes, "公共服务详情", &d.public_service);
    out.push(ScoreCard::new("IP 属性", attributes));
    let mut safety = Vec::new();
    for (name, value) in [
        ("VPN", d.risk.is_vpn),
        ("代理", d.risk.is_proxy),
        ("Tor", d.risk.is_tor),
        ("爬虫/机器人", d.risk.is_crawler),
        ("历史滥用", d.risk.is_abuser),
        ("Reddit 封锁", d.reddit_blocked),
    ] {
        safety.push(signal(name, value, THEME_ERROR));
    }
    append_value(&mut safety, "滥用分", &d.abuser_score);
    append_value(&mut safety, "风险情报", &d.intelligence);
    append_value(&mut safety, "VPN 轨迹", &d.vpn_trace);
    out.push(ScoreCard::new("安全信号", safety));
    let mut scenes = Vec::new();
    for scene in [Scene::TikTok, Scene::Social, Scene::Ai] {
        if let Some(score) = scene::scene_score(d, scene) {
            scenes.push(Line::from(vec![
                Span::raw(format!("{} {}/10 ", scene.name(), score.score)),
                badge(
                    score.verdict,
                    if score.score >= 8 {
                        THEME_SUCCESS
                    } else if score.score >= 5 {
                        THEME_WARNING
                    } else {
                        THEME_ERROR
                    },
                ),
            ]));
            if !score.tip.is_empty() {
                scenes.push(styled(score.tip, THEME_WARNING));
            }
        } else {
            scenes.push(kv(scene.name(), "信任分缺失，无法计算"));
        }
    }
    out.push(ScoreCard::new("场景评分", scenes));
    let mut geo = Vec::new();
    if let Some((lat, lon)) = d.best_coordinates() {
        geo.push(kv("最佳坐标", &format!("{lat}, {lon}")));
    }
    for g in &d.geo_sources {
        geo.push(kv(
            &g.src,
            &format!(
                "{} {} {} {}",
                flag_emoji(&g.country_code),
                g.country,
                g.region,
                g.city
            ),
        ));
        if let Some((lat, lon)) = g.lat.zip(g.lon) {
            geo.push(kv("坐标", &format!("{lat}, {lon}")));
        }
        if let Some(accuracy) = g.accuracy_km {
            geo.push(kv("定位精度", &format!("{accuracy} km")));
        }
    }
    if geo.is_empty() {
        geo.push(styled("暂无多源定位记录", THEME_MUTED));
    }
    out.push(ScoreCard::new("多源定位 · g1 > g7 > g3 > g2", geo));
    for (title, records) in [
        ("位置历史", &d.location_history),
        ("ASN 历史", &d.asn_history),
        ("公司历史", &d.company_history),
        ("机房邻居", &d.dc_neighbors),
    ] {
        let mut lines = Vec::new();
        append_records(&mut lines, "记录", records);
        out.push(ScoreCard::new(title, lines));
    }
}

fn signal(name: &str, value: Option<bool>, yes_color: Color) -> Line<'static> {
    let (label, color) = match value {
        Some(true) => ("是", yes_color),
        Some(false) => ("否", THEME_SUCCESS),
        None => ("未知", THEME_MUTED),
    };
    Line::from(vec![Span::raw(format!("{name}：")), badge(label, color)])
}

fn bgp_node(n: &BgpNode) -> Vec<Line<'static>> {
    let mut lines = vec![kv(
        &format!("AS{} {}", n.asn, n.name),
        &format!(
            "路径占比 {:.2}%{}",
            n.share,
            if n.tier1 { " · Tier 1" } else { "" }
        ),
    )];
    if let Some(via) = n.via {
        lines.push(kv("经由", &format!("AS{via}")));
    }
    lines
}

fn append_records(lines: &mut Vec<Line<'static>>, name: &str, records: &[Value]) {
    if records.is_empty() {
        lines.push(styled(format!("{name}：暂无记录"), THEME_MUTED));
    }
    for (i, value) in records.iter().enumerate() {
        lines.push(styled(format!("{name} {}", i + 1), THEME_ACCENT));
        append_value(lines, "", value);
    }
}

fn append_value(lines: &mut Vec<Line<'static>>, name: &str, value: &Value) {
    match value {
        Value::Null => {}
        Value::Object(fields) => {
            if !name.is_empty() {
                lines.push(styled(name, THEME_ACCENT));
            }
            for (key, value) in fields {
                if matches!(key.as_str(), "seen_at" | "taken_at")
                    && let Some(seconds) = value.as_u64().or_else(|| value.as_str()?.parse().ok())
                {
                    lines.push(kv(
                        "观测时间（北京时间）",
                        &crate::history::format_recorded_at(seconds.saturating_mul(1000)),
                    ));
                } else {
                    append_value(lines, field_label(key), value);
                }
            }
        }
        Value::Array(values) => {
            if !values.is_empty() {
                lines.push(styled(name, THEME_ACCENT));
            }
            for value in values {
                append_value(lines, "", value);
            }
        }
        Value::String(s) => {
            if !s.is_empty() {
                lines.push(kv(name, s));
            }
        }
        _ => lines.push(kv(name, &value.to_string())),
    }
}

fn field_label(key: &str) -> &str {
    match key {
        "seen_at" | "taken_at" => "观测时间",
        "country" => "国家",
        "country_code" => "国别码",
        "region" => "地区",
        "city" => "城市",
        "asn" => "ASN",
        "asn_org" => "ASN 组织",
        "company" | "company_name" | "name" => "名称",
        "ip" => "IP",
        "cidr" | "prefix" => "网段",
        "first" => "起始地址",
        "last" => "结束地址",
        "count" => "数量",
        "abuser_level" => "滥用档位",
        "abuser_score_raw" => "原始滥用分",
        "rep_threat" | "httpbl_threat" => "蜜罐风险值",
        "threats" => "风险项",
        "service" => "服务",
        "operator" => "运营商",
        "service_type" => "服务类型",
        "note" => "说明",
        "upstreams" => "上游",
        "second" => "二级上游",
        "share" => "路径占比（%）",
        "via" => "经由 ASN",
        "day" => "日期",
        "v" => "数值",
        "i" => "索引",
        "tier1" => "Tier 1",
        "type" => "类型",
        _ => key,
    }
}
