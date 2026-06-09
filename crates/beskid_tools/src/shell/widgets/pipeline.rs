use ratatui::Frame;
use ratatui::layout::Rect;
use ratkit::services::hotkey_service::Hotkey;

use crate::pipeline::tui::stage_focus::StageFocus;
use crate::pipeline::tui::widgets::{
    draw_context_bar, draw_pipeline_tree, draw_progress_footer, draw_stage_panel,
};
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct HeaderWidget;

impl BeskidWidget for HeaderWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.header",
            title: "Header",
            icon: "—",
        }
    }
    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }
    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let focus = StageFocus::from_shell_state(ctx.shell_state);
        draw_context_bar(frame, area, ctx.shell_state, focus);
    }
}

pub struct StageWidget;

impl BeskidWidget for StageWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "pipeline.stage",
            title: "Stage",
            icon: "◆",
        }
    }
    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }
    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let focus = StageFocus::from_shell_state(ctx.shell_state);
        draw_stage_panel(frame, area, ctx.shell_state, focus);
    }
}

pub struct DetailWidget;

impl BeskidWidget for DetailWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "pipeline.detail",
            title: "Detail",
            icon: "▤",
        }
    }
    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }
    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let focus = StageFocus::from_shell_state(ctx.shell_state);
        let title = focus.title();
        draw_pipeline_tree(
            frame,
            area,
            &ctx.shell_state.tree_nodes,
            &mut ctx.shell_state.tree_state,
            title,
        );
    }
}

pub struct FooterWidget;

impl BeskidWidget for FooterWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "pipeline.footer",
            title: "Progress",
            icon: "▬",
        }
    }
    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }
    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        draw_progress_footer(frame, area, &ctx.shell_state.pipeline);
    }
}
