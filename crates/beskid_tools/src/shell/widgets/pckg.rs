use ratatui::Frame;
use ratatui::layout::Rect;
use crate::shell::primitives::Hotkey;

use crate::shell::catalog::ContextualCommand;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};
use crate::tui::screens::pckg_overlay;
use crate::tui::shell::focus::OverlayKind;

pub struct PckgWidget;

impl BeskidWidget for PckgWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "pckg.browser",
            title: "Packages",
            icon: "📦",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn contextual_commands(&self, _ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        vec![ContextualCommand {
            id: "ctx.pckg",
            name: "Browse packages",
            description: "Open pckg registry browser",
            icon: "📦",
            args_hint: Some("<query>"),
            widget_id: Some("pckg.browser"),
        }]
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        if !ctx.shell_state.pckg.catalog_loaded {
            ctx.shell_state.pckg.pending_catalog_refresh = true;
        }
        pckg_overlay::render(area, frame, ctx.shell_state);
    }
}

pub fn open_pckg(ctx: &mut WidgetContext<'_>) {
    ctx.shell_state.set_overlay_visible(OverlayKind::Pckg, true);
    ctx.shell_state.focus_overlay(OverlayKind::Pckg);
    if !ctx.shell_state.pckg.catalog_loaded {
        ctx.shell_state.pckg.pending_catalog_refresh = true;
    }
}
