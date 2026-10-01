//! 将完整内容画布的可见行复制到终端，保留卡片在视口外的内容。
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
