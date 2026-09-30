//! 图标表：全项目图标的统一收口。
//!
//! 使用 Nerd Font 码位（v3.5.1），品牌图标用 md-*（Material Design）字形近似；
//! 国旗不用此表（按规格用 emoji）。码位权威依据：nerd-fonts 官方 glyphnames.json。

/// 项目用到的全部图标；新图标只允许加在这里。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    // 七个功能页
    IpQuery,
    Claude,
    Gpt,
    IpScore,
    DnsLeak,
    WebRtc,
    Connectivity,
    // 状态语义
    Check,
    Warning,
    Error,
    Info,
    Help,
    // 对象与动作
    Shield,
    Search,
    Server,
    Earth,
    Flag,
    Clock,
    Lock,
    Vpn,
    Network,
    Ethernet,
    Radar,
    Gauge,
    Refresh,
    Terminal,
    Connection,
    Fingerprint,
    Key,
    Web,
}

/// 全量图标，供穷举校验与测试。
pub const ALL: [Icon; 30] = [
    Icon::IpQuery,
    Icon::Claude,
    Icon::Gpt,
    Icon::IpScore,
    Icon::DnsLeak,
    Icon::WebRtc,
    Icon::Connectivity,
    Icon::Check,
    Icon::Warning,
    Icon::Error,
    Icon::Info,
    Icon::Help,
    Icon::Shield,
    Icon::Search,
    Icon::Server,
    Icon::Earth,
    Icon::Flag,
    Icon::Clock,
    Icon::Lock,
    Icon::Vpn,
    Icon::Network,
    Icon::Ethernet,
    Icon::Radar,
    Icon::Gauge,
    Icon::Refresh,
    Icon::Terminal,
    Icon::Connection,
    Icon::Fingerprint,
    Icon::Key,
    Icon::Web,
];

/// 图标字形（单个 Nerd Font 字符）。
pub fn glyph(icon: Icon) -> char {
    match icon {
        Icon::IpQuery => '\u{f0a5f}',     // md-ip
        Icon::Claude => '\u{f06a9}',      // md-robot（Claude 品牌近似）
        Icon::Gpt => '\u{f167a}',         // md-robot_outline（GPT 品牌近似）
        Icon::IpScore => '\u{f04ce}',     // md-star
        Icon::DnsLeak => '\u{f01d6}',     // md-dns
        Icon::WebRtc => '\u{f1248}',      // md-webrtc
        Icon::Connectivity => '\u{f0318}', // md-lan_connect
        Icon::Check => '\u{f05e0}',       // md-check_circle
        Icon::Warning => '\u{f0028}',     // md-alert_circle
        Icon::Error => '\u{f0159}',       // md-close_circle
        Icon::Info => '\u{f02fc}',        // md-information
        Icon::Help => '\u{f02d7}',        // md-help_circle
        Icon::Shield => '\u{f0565}',      // md-shield_check
        Icon::Search => '\u{f0349}',      // md-magnify
        Icon::Server => '\u{f048b}',      // md-server
        Icon::Earth => '\u{f01e7}',       // md-earth
        Icon::Flag => '\u{f023b}',        // md-flag
        Icon::Clock => '\u{f0150}',       // md-clock_outline
        Icon::Lock => '\u{f033e}',        // md-lock
        Icon::Vpn => '\u{f0582}',         // md-vpn
        Icon::Network => '\u{f06f3}',     // md-network
        Icon::Ethernet => '\u{f0200}',    // md-ethernet
        Icon::Radar => '\u{f0437}',       // md-radar
        Icon::Gauge => '\u{f029a}',       // md-gauge
        Icon::Refresh => '\u{f0450}',     // md-refresh
        Icon::Terminal => '\u{f07b7}',    // md-console_line
        Icon::Connection => '\u{f1616}',  // md-connection
        Icon::Fingerprint => '\u{f0237}', // md-fingerprint
        Icon::Key => '\u{f0306}',         // md-key
        Icon::Web => '\u{f059f}',         // md-web
    }
}

/// 「图标 + 空格 + 文案」的组合串，供标题与导航使用。
pub fn labeled(icon: Icon, text: &str) -> String {
    format!("{} {}", glyph(icon), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_code_points_from_glyphnames_json() {
        // 以下码位逐项转录自 nerd-fonts v3.5.1 glyphnames.json（独立事实源）
        assert_eq!(glyph(Icon::IpQuery), '\u{f0a5f}'); // md-ip
        assert_eq!(glyph(Icon::Claude), '\u{f06a9}'); // md-robot
        assert_eq!(glyph(Icon::Gpt), '\u{f167a}'); // md-robot_outline
        assert_eq!(glyph(Icon::IpScore), '\u{f04ce}'); // md-star
        assert_eq!(glyph(Icon::DnsLeak), '\u{f01d6}'); // md-dns
        assert_eq!(glyph(Icon::WebRtc), '\u{f1248}'); // md-webrtc
        assert_eq!(glyph(Icon::Connectivity), '\u{f0318}'); // md-lan_connect
        assert_eq!(glyph(Icon::Check), '\u{f05e0}'); // md-check_circle
        assert_eq!(glyph(Icon::Warning), '\u{f0028}'); // md-alert_circle
        assert_eq!(glyph(Icon::Error), '\u{f0159}'); // md-close_circle
        assert_eq!(glyph(Icon::Info), '\u{f02fc}'); // md-information
        assert_eq!(glyph(Icon::Shield), '\u{f0565}'); // md-shield_check
    }

    #[test]
    fn every_icon_is_a_nerd_font_private_use_char() {
        for icon in ALL {
            let c = glyph(icon);
            let code = c as u32;
            assert!(
                (0xe000..=0xf8ff).contains(&code) || (0xf_0000..=0xf_fffd).contains(&code),
                "{icon:?} 的码位 U+{code:04X} 不在 Nerd Font 私有区"
            );
            assert_ne!(c, ' ');
        }
    }

    #[test]
    fn all_variants_are_listed() {
        // ALL 与 enum 变体一一对应：长度一致且互不重复
        assert_eq!(ALL.len(), 30);
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn labeled_joins_glyph_and_text() {
        assert_eq!(labeled(Icon::WebRtc, "WebRTC"), "\u{f1248} WebRTC");
    }
}
