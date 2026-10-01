//! 将完整内容画布的可见行复制到终端，保留卡片在视口外的内容。
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::ui::layout::measured_card_rects;

/// 查看状态的统一滚动键；输入焦点由调用方先行消费。
pub fn key_scroll(current: u16, key: crossterm::event::KeyCode, max: u16) -> Option<u16> {
    use crossterm::event::KeyCode;
    let current = current.min(max);
    Some(match key {
        KeyCode::Up => current.saturating_sub(1),
        KeyCode::Down => current.saturating_add(1).min(max),
        KeyCode::PageUp => current.saturating_sub(10),
        KeyCode::PageDown => current.saturating_add(10).min(max),
        KeyCode::Home => 0,
        KeyCode::End => max,
        _ => return None,
    })
}

/// 每个滚轮事件移动一行，与上下键使用相同边界。
pub fn mouse_scroll(current: u16, kind: crossterm::event::MouseEventKind, max: u16) -> Option<u16> {
    use crossterm::event::{KeyCode, MouseEventKind};
    key_scroll(
        current,
        match kind {
            MouseEventKind::ScrollUp => KeyCode::Up,
            MouseEventKind::ScrollDown => KeyCode::Down,
            _ => return None,
        },
        max,
    )
}

/// 卡片按换行后的正文高度铺满画布，不受终端视口高度限制。
pub fn card_canvas(width: u16, cards: Vec<Paragraph<'static>>) -> Buffer {
    let cards: Vec<_> = cards
        .into_iter()
        .map(|p| p.wrap(Wrap { trim: false }))
        .collect();
    let (rects, height) = measured_card_rects(width, cards.len(), |i, card_width| {
        cards[i].line_count(card_width).min(usize::from(u16::MAX)) as u16
    });
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    for (paragraph, rect) in cards.into_iter().zip(rects) {
        paragraph.render(rect, &mut buffer);
    }
    buffer
}

pub fn copy_viewport(source: &Buffer, destination: &mut Buffer, area: Rect, scroll: u16) {
    for y in 0..area.height {
        let source_y = scroll.saturating_add(y);
        if source_y >= source.area.height {
            break;
        }
        for x in 0..area.width.min(source.area.width) {
            destination[(area.x + x, area.y + y)] = source[(x, source_y)].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::key_scroll;
    use crossterm::event::KeyCode;

    #[test]
    fn wheel_moves_immediately_and_clamps_at_both_ends() {
        use crossterm::event::MouseEventKind::*;
        assert_eq!(super::mouse_scroll(3, ScrollDown, 8), Some(4));
        assert_eq!(super::mouse_scroll(3, ScrollUp, 8), Some(2));
        assert_eq!(super::mouse_scroll(0, ScrollUp, 8), Some(0));
        assert_eq!(super::mouse_scroll(8, ScrollDown, 8), Some(8));
        assert_eq!(super::mouse_scroll(3, Moved, 8), None);
    }

    #[test]
    fn scroll_keys_reach_page_end_without_leaving_content() {
        assert_eq!(key_scroll(3, KeyCode::PageDown, 8), Some(8));
        assert_eq!(key_scroll(3, KeyCode::End, 80), Some(80));
        assert_eq!(key_scroll(8, KeyCode::PageUp, 8), Some(0));
        assert_eq!(key_scroll(0, KeyCode::Up, 8), Some(0));
        assert_eq!(key_scroll(8, KeyCode::Down, 8), Some(8));
        assert_eq!(key_scroll(3, KeyCode::Home, 8), Some(0));
        assert_eq!(key_scroll(3, KeyCode::Char('q'), 8), None);
    }
}
