use ratatui::Frame;
use ratatui::layout::Rect;
use crate::shell::primitives::Hotkey;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};
use crate::shell::widgets::compile_debug::{draw_compile_debug_panel, CompileDebugTab};

/// Debugger page — surfaces the compile debugger timeline (same data as compile/debug page).
pub struct DebugFutureWidget;

impl BeskidWidget for DebugFutureWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "debug.future",
            title: "Debugger",
            icon: "◉",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        draw_compile_debug_panel(frame, area, ctx.shell_state, CompileDebugTab::Timeline);
    }
}
