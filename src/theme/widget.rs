//! 主题组件层：卡片、徽章、key-value 行、渐变信任分条。
//!
//! 页面代码不得直接使用 `Block::bordered()` 与裸 `Color::*`，
//! 一律经由这里的助手与 `THEME_*` 常量组合出界面。

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding};

use super::color::{THEME_BADGE_FG, THEME_BORDER, THEME_MUTED, THEME_TEXT, trust_bar_cells};

/// 卡片：带边框与标题的分区，是界面的基本布局单位。
pub fn card<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::bordered()
        .border_style(Style::new().fg(THEME_BORDER))
        .title(title.into().style(Style::new().fg(THEME_TEXT)))
        .padding(Padding::horizontal(1))
}

/// 徽章：语义底色短标签，前后各留一个空格作内边距。
pub fn badge(text: &str, semantic: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::new().bg(semantic).fg(THEME_BADGE_FG),
    )
}

/// key-value 行：键暗灰、值正文色。
pub fn kv(key: &str, value: &str) -> Line<'static> {
    kv_styled(key, value, Style::new().fg(THEME_TEXT))
}

/// key-value 行（指定值的样式）。
pub fn kv_styled(key: &str, value: &str, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key}："), Style::new().fg(THEME_MUTED)),
        Span::styled(value.to_string(), value_style),
    ])
}

/// 渐变信任分条：已覆盖格按 0-100 刻度取渐变色，未覆盖格暗灰。
pub fn trust_bar(score: u8, width: u16) -> Line<'static> {
    Line::from(
        trust_bar_cells(score, width)
            .into_iter()
            .map(|cell| {
                Span::styled(
                    if cell.filled { "█" } else { "░" },
                    Style::new().fg(cell.color),
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::color::{
        THEME_ACCENT, THEME_ERROR, THEME_SUCCESS, THEME_WARNING, trust_gradient_at,
    };

    #[test]
    fn badge_has_pill_padding_and_semantic_bg() {
        let span = badge("纯净", THEME_SUCCESS);
        assert_eq!(span.content, " 纯净 ");
        assert_eq!(span.style.bg, Some(THEME_SUCCESS));
        assert_eq!(span.style.fg, Some(THEME_BADGE_FG));
    }

    #[test]
    fn badge_accepts_each_semantic_color() {
        for semantic in [THEME_SUCCESS, THEME_WARNING, THEME_ERROR, THEME_ACCENT] {
            let span = badge("未检测到", semantic);
            assert_eq!(span.style.bg, Some(semantic));
        }
    }

    #[test]
    fn kv_key_is_muted_and_value_is_plain() {
        let line = kv("归属地", "日本 东京");
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "归属地：");
        assert_eq!(line.spans[0].style.fg, Some(THEME_MUTED));
        assert_eq!(line.spans[1].content, "日本 东京");
        assert_eq!(line.spans[1].style.fg, Some(THEME_TEXT));
    }

    #[test]
    fn kv_styled_applies_value_style() {
        let line = kv_styled("状态", "泄漏", Style::new().fg(THEME_ERROR));
        assert_eq!(line.spans[0].content, "状态：");
        assert_eq!(line.spans[1].style.fg, Some(THEME_ERROR));
    }

    #[test]
    fn trust_bar_uses_block_glyphs_and_gradient_colors() {
        let line = trust_bar(50, 10);
        assert_eq!(line.spans.len(), 10);
        let chars: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(chars, "█████░░░░░");
        assert_eq!(line.spans[0].style.fg, Some(trust_gradient_at(0.0)));
        assert_eq!(line.spans[9].style.fg, Some(THEME_MUTED));
    }

    #[test]
    fn trust_bar_zero_score_is_all_dim() {
        let line = trust_bar(0, 6);
        let chars: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(chars, "░░░░░░");
    }

    #[test]
    fn trust_bar_full_score_is_all_gradient() {
        let line = trust_bar(100, 4);
        let chars: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(chars, "████");
        assert_eq!(line.spans[3].style.fg, Some(trust_gradient_at(1.0)));
    }
}
