use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::pipeline::tui::widgets::draw_tabbed_log_panel;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct LogWidget;

impl BeskidWidget for LogWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta { id: "shell.log", title: "Log", icon: "≡" }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        render_log_panel(area, frame, ctx);
    }
}

pub struct LogPanelWidget;

impl BeskidWidget for LogPanelWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta { id: "pipeline.log", title: "Build log", icon: "≡" }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        render_log_panel(area, frame, ctx);
    }
}

fn render_log_panel(area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
    if ctx.scope.is_user() && !ctx.shell_state.pipeline_active() {
        frame.render_widget(Paragraph::new(ShellScope::no_project_lines(&ctx.key_bindings.palette_hint())), area);
        return;
    }
    draw_tabbed_log_panel(frame, area, ctx.shell_state.log_tab, &mut ctx.shell_state.log_states);
}
