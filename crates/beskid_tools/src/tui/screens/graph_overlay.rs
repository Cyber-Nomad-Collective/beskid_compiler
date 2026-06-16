//! Dependency graph modal overlay.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::context::WidgetContext;
use crate::shell::widgets::draw_graph_deps_panel;

pub fn render(area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
    draw_graph_deps_panel(area, frame, ctx);
}
