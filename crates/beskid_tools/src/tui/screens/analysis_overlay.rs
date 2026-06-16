//! Analysis diagnostics modal overlay.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::context::WidgetContext;
use crate::shell::widgets::draw_analysis_panel;

pub fn render(area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
    draw_analysis_panel(area, frame, ctx);
}
