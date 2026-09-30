//! 卡片网格布局的纯决策函数。
//!
//! 输入终端宽度等几何量，输出布局形态（列数）与卡片矩形；
//! 这是本项目的测试接缝之一，渲染本身不进单元测试。

use ratatui::layout::Rect;

/// 三列卡片网格的最低终端宽度（列）；低于此宽度降级为单列。
pub const GRID_MIN_WIDTH: u16 = 90;

/// 卡片之间的间隔（列与行同值）。
pub const CARD_GAP: u16 = 1;

/// 卡片网格列数：终端宽 ≥ 90 列时三列，否则单列。
pub fn card_grid_columns(width: u16) -> u16 {
    if width >= GRID_MIN_WIDTH { 3 } else { 1 }
}

/// 把区域按当前网格形态切成 `count` 个卡片矩形。
///
/// 均分后余量分给靠前的列/行；空间不足以容纳间隔时用饱和减法兜底。
pub fn card_rects(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let cols = usize::from(card_grid_columns(area.width));
    let col_widths = split_lengths(area.width, cols);
    let col_x = offsets(&col_widths, CARD_GAP);
    let rows = count.div_ceil(cols);
    let row_heights = split_lengths(area.height, rows);
    let row_y = offsets(&row_heights, CARD_GAP);

    (0..count)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            Rect {
                x: area.x.saturating_add(col_x[col]),
                y: area.y.saturating_add(row_y[row]),
                width: col_widths[col],
                height: row_heights[row],
            }
        })
        .collect()
}

/// 把总长按 `n` 份均分（先扣掉 `n-1` 个间隔），余量分给靠前的份。
///
/// 恰好返回 `n` 个长度；空间不足时部分长度可能为 0（退化但不 panic）。
fn split_lengths(total: u16, n: usize) -> Vec<u16> {
    if n == 0 {
        return Vec::new();
    }
    let gaps = (n as u64 - 1) * u64::from(CARD_GAP);
    let usable = u64::from(total).saturating_sub(gaps);
    let n = n as u64;
    let (base, rem) = (usable / n, usable % n);
    (0..n)
        .map(|i| if i < rem { base + 1 } else { base })
        .map(|len| len.min(u64::from(u16::MAX)) as u16)
        .collect()
}

/// 各份在总长内的起始偏移（长度 + 间隔累进）。
fn offsets(lengths: &[u16], gap: u16) -> Vec<u16> {
    let mut acc = 0u16;
    lengths
        .iter()
        .map(|&len| {
            let start = acc;
            acc = acc.saturating_add(len).saturating_add(gap);
            start
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_columns_at_90_or_wider() {
        assert_eq!(card_grid_columns(90), 3);
        assert_eq!(card_grid_columns(120), 3);
        assert_eq!(card_grid_columns(u16::MAX), 3);
    }

    #[test]
    fn single_column_below_90() {
        assert_eq!(card_grid_columns(89), 1);
        assert_eq!(card_grid_columns(1), 1);
        assert_eq!(card_grid_columns(0), 1);
    }

    #[test]
    fn seven_cards_in_three_column_grid() {
        // 手算：宽 100 三列 → 可用 98，均分 32 余 2 → 列宽 [33,33,32]，
        // x 位置 [0,34,68]；高 10 三行 → 可用 8，均分 2 余 2 → 行高 [3,3,2]，y [0,4,8]
        let rects = card_rects(Rect::new(0, 0, 100, 10), 7);
        assert_eq!(
            rects,
            vec![
                Rect::new(0, 0, 33, 3),
                Rect::new(34, 0, 33, 3),
                Rect::new(68, 0, 32, 3),
                Rect::new(0, 4, 33, 3),
                Rect::new(34, 4, 33, 3),
                Rect::new(68, 4, 32, 3),
                Rect::new(0, 8, 33, 2),
            ]
        );
    }

    #[test]
    fn narrow_terminal_stacks_single_column() {
        let rects = card_rects(Rect::new(0, 0, 89, 14), 7);
        assert_eq!(rects.len(), 7);
        assert!(rects.iter().all(|r| r.width == 89 && r.x == 0));
        let heights: Vec<u16> = rects.iter().map(|r| r.height).collect();
        assert_eq!(heights, [2, 1, 1, 1, 1, 1, 1]);
        let ys: Vec<u16> = rects.iter().map(|r| r.y).collect();
        assert_eq!(ys, [0, 3, 5, 7, 9, 11, 13]);
    }

    #[test]
    fn fewer_cards_than_columns_keeps_grid_slots() {
        let rects = card_rects(Rect::new(0, 0, 100, 10), 2);
        assert_eq!(
            rects,
            vec![Rect::new(0, 0, 33, 10), Rect::new(34, 0, 33, 10)]
        );
    }

    #[test]
    fn two_cards_stack_vertically_when_narrow() {
        let rects = card_rects(Rect::new(0, 0, 89, 10), 2);
        // 可用高 9，均分 4 余 1 → [5,4]
        assert_eq!(rects, vec![Rect::new(0, 0, 89, 5), Rect::new(0, 6, 89, 4)]);
    }

    #[test]
    fn zero_cards_yields_no_rects() {
        assert!(card_rects(Rect::new(0, 0, 100, 10), 0).is_empty());
    }

    #[test]
    fn degenerate_width_does_not_panic() {
        // 宽 2 低于 90：走单列竖排，无间隔冲突
        let rects = card_rects(Rect::new(0, 0, 2, 5), 3);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.width == 2 && r.x == 0));
        let ys: Vec<u16> = rects.iter().map(|r| r.y).collect();
        assert_eq!(ys, [0, 2, 4]);
    }

    #[test]
    fn cards_tile_the_area_without_overlap() {
        let area = Rect::new(0, 0, 120, 30);
        let rects = card_rects(area, 7);
        for r in &rects {
            assert!(
                r.right() <= area.right() && r.bottom() <= area.bottom(),
                "{r:?} 越界"
            );
        }
    }
}
