//! Viewer content-area geometry for the code widget.

use ratatui::layout::Rect;

/// Returns the viewer content area before the statusline.
pub(crate) fn viewer_content_area(area: Rect) -> Rect {
    if area.height > 1 {
        Rect {
            height: area.height.saturating_sub(1),
            ..area
        }
    } else {
        area
    }
}
