//! Analysis diagnostics modal overlay.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::context::WidgetContext;
use crate::shell::widgets::draw_build_report;

pub fn render(area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
    draw_build_report(area, frame, ctx);
}
