use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::catalog::ContextualCommand;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};
use crate::tui::screens::tests_overlay;
use crate::tui::shell::focus::OverlayKind;

pub struct TestsWidget;

impl BeskidWidget for TestsWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "tests.runner",
            title: "Tests",
            icon: "✓",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        match ctx.scope {
            ShellScope::User => Vec::new(),
            _ => vec![ContextualCommand {
                id: "ctx.tests",
                name: "Run tests",
                description: "Open tests overlay",
                icon: "✓",
                args_hint: None,
                widget_id: Some("tests.runner"),
            }],
        }
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        if ctx.shell_state.overlay_visible(OverlayKind::Tests) {
            tests_overlay::render(area, frame, ctx.shell_state);
        }
    }
}

pub fn open_tests(ctx: &mut WidgetContext<'_>) {
    ctx.shell_state
        .set_overlay_visible(OverlayKind::Tests, true);
    ctx.shell_state.focus_overlay(OverlayKind::Tests);
    ctx.shell_state.sync_code_viewer_for_selection();
}
