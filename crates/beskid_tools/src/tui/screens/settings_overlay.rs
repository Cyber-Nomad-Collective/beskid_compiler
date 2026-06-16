//! Settings modal overlay.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::context::WidgetContext;
use crate::shell::registry::WidgetRegistry;

pub fn render(
    area: Rect,
    frame: &mut Frame,
    ctx: &mut WidgetContext<'_>,
    registry: &WidgetRegistry,
) {
    if let Some(widget) = registry.get("shell.settings") {
        widget.render(area, frame, ctx);
    }
}
