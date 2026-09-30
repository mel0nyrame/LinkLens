//! 语义色常量与信任分渐变的纯逻辑。
//!
//! 语义五色约定：绿=结论 / 黄=警示·冷却·配额 / 红=错误·泄漏 / 青=强调 / 暗灰=次要。
//! 页面代码不得出现裸 `Color::*`，一律引用 `THEME_*`。

use ratatui::style::Color;

/// 结论 / 通过 / 纯净。
pub const THEME_SUCCESS: Color = Color::Rgb(0x4a, 0xde, 0x80);
/// 警示 / 冷却 / 配额紧张。
pub const THEME_WARNING: Color = Color::Rgb(0xfa, 0xcc, 0x15);
/// 错误 / 泄漏 / 高危。
pub const THEME_ERROR: Color = Color::Rgb(0xf8, 0x71, 0x71);
/// 强调 / 交互提示。
pub const THEME_ACCENT: Color = Color::Rgb(0x22, 0xd3, 0xee);
/// 次要信息 / 弱化文案。
pub const THEME_MUTED: Color = Color::Rgb(0x71, 0x71, 0x7a);

/// 正文前景色。
pub const THEME_TEXT: Color = Color::Rgb(0xe4, 0xe4, 0xe7);
/// 卡片边框色。
pub const THEME_BORDER: Color = Color::Rgb(0x3f, 0x3f, 0x46);
/// 徽章语义底色上的前景色。
pub const THEME_BADGE_FG: Color = Color::Rgb(0x09, 0x09, 0x0b);

/// 渐变信任分条三停点：低分红 → 中段黄 → 高分绿，与语义色同族。
const GRADIENT_LOW: (u8, u8, u8) = (0xf8, 0x71, 0x71);
const GRADIENT_MID: (u8, u8, u8) = (0xfa, 0xcc, 0x15);
const GRADIENT_HIGH: (u8, u8, u8) = (0x4a, 0xde, 0x80);

/// 信任分条上 `t`（0.0=0 分，1.0=100 分）位置的渐变色；超出 [0,1] 一律夹紧。
///
/// 分段线性插值：[0, 0.5] 红→黄，[0.5, 1] 黄→绿，通道值向零截断。
pub fn trust_gradient_at(t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (from, to, frac) = if t <= 0.5 {
        (GRADIENT_LOW, GRADIENT_MID, t / 0.5)
    } else {
        (GRADIENT_MID, GRADIENT_HIGH, (t - 0.5) / 0.5)
    };
    Color::Rgb(
        lerp(from.0, to.0, frac),
        lerp(from.1, to.1, frac),
        lerp(from.2, to.2, frac),
    )
}

fn lerp(from: u8, to: u8, frac: f64) -> u8 {
    (f64::from(from) + (f64::from(to) - f64::from(from)) * frac) as u8
}

/// 渐变信任分条的单格：`filled` 表示该格已被信任分覆盖。
///
/// 颜色编码格子在 0-100 刻度上的绝对位置（而非已覆盖比例），未覆盖格统一暗灰。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BarCell {
    pub color: Color,
    pub filled: bool,
}

/// 信任分条逐格取色：`score` 超过 100 按 100 处理，`width` 为 0 时返回空。
pub fn trust_bar_cells(score: u8, width: u16) -> Vec<BarCell> {
    let fill = u32::from(score.min(100)) * u32::from(width) / 100;
    let denom = u32::from(width.saturating_sub(1)).max(1);
    (0..u32::from(width))
        .map(|i| BarCell {
            color: if i < fill {
                trust_gradient_at(f64::from(i) / f64::from(denom))
            } else {
                THEME_MUTED
            },
            filled: i < fill,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_palette_is_pinned() {
        assert_eq!(THEME_SUCCESS, Color::Rgb(0x4a, 0xde, 0x80));
        assert_eq!(THEME_WARNING, Color::Rgb(0xfa, 0xcc, 0x15));
        assert_eq!(THEME_ERROR, Color::Rgb(0xf8, 0x71, 0x71));
        assert_eq!(THEME_ACCENT, Color::Rgb(0x22, 0xd3, 0xee));
        assert_eq!(THEME_MUTED, Color::Rgb(0x71, 0x71, 0x7a));
        // 语义五色互不相同
        let palette = [
            THEME_SUCCESS,
            THEME_WARNING,
            THEME_ERROR,
            THEME_ACCENT,
            THEME_MUTED,
        ];
        for (i, a) in palette.iter().enumerate() {
            for b in &palette[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn gradient_hits_three_stops() {
        assert_eq!(trust_gradient_at(0.0), Color::Rgb(0xf8, 0x71, 0x71));
        assert_eq!(trust_gradient_at(0.5), Color::Rgb(0xfa, 0xcc, 0x15));
        assert_eq!(trust_gradient_at(1.0), Color::Rgb(0x4a, 0xde, 0x80));
    }

    #[test]
    fn gradient_interpolates_linearly() {
        // 0.25 是红→黄的中点；0.75 是黄→绿的中点（u8 截断）
        assert_eq!(trust_gradient_at(0.25), Color::Rgb(0xf9, 0x9e, 0x43));
        assert_eq!(trust_gradient_at(0.75), Color::Rgb(0xa2, 0xd5, 0x4a));
    }

    #[test]
    fn gradient_clamps_out_of_range() {
        assert_eq!(trust_gradient_at(-0.5), Color::Rgb(0xf8, 0x71, 0x71));
        assert_eq!(trust_gradient_at(1.5), Color::Rgb(0x4a, 0xde, 0x80));
    }

    #[test]
    fn trust_bar_zero_score_is_all_muted() {
        let cells = trust_bar_cells(0, 10);
        assert_eq!(cells.len(), 10);
        assert!(cells.iter().all(|c| !c.filled && c.color == THEME_MUTED));
    }

    #[test]
    fn trust_bar_full_score_is_all_gradient() {
        let cells = trust_bar_cells(100, 10);
        assert_eq!(cells.len(), 10);
        assert!(cells.iter().all(|c| c.filled));
        assert_eq!(cells[0].color, Color::Rgb(0xf8, 0x71, 0x71));
        assert_eq!(cells[9].color, Color::Rgb(0x4a, 0xde, 0x80));
    }

    #[test]
    fn trust_bar_half_score_fills_first_half() {
        let cells = trust_bar_cells(50, 10);
        assert_eq!(cells.len(), 10);
        assert!(cells[..5].iter().all(|c| c.filled));
        assert!(
            cells[5..]
                .iter()
                .all(|c| !c.filled && c.color == THEME_MUTED)
        );
    }

    #[test]
    fn trust_bar_fill_uses_integer_floor() {
        // 33 * 10 / 100 = 3
        let cells = trust_bar_cells(33, 10);
        assert_eq!(cells.iter().filter(|c| c.filled).count(), 3);
        // 7 * 3 / 100 = 0
        assert!(trust_bar_cells(7, 3).iter().all(|c| !c.filled));
    }

    #[test]
    fn trust_bar_clamps_score_and_handles_tiny_width() {
        assert!(trust_bar_cells(255, 10).iter().all(|c| c.filled));
        assert!(trust_bar_cells(50, 0).is_empty());
        assert_eq!(trust_bar_cells(50, 1).len(), 1);
    }
}
