//! `BeskidWidget` trait and shell action types.

use super::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::catalog::ContextualCommand;
use super::context::WidgetContext;
use super::input::ShellInput;

#[derive(Debug, Clone)]
pub struct WidgetMeta {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAction {
    None,
    Redraw,
    Quit,
    OpenPalette,
    OpenOverlay(&'static str),
    RunContextual(&'static str),
}

/// Pluggable shell tile rendered inside a board region.
pub trait BeskidWidget: Send {
    fn meta(&self) -> WidgetMeta;
    fn hotkeys(&self, ctx: &WidgetContext<'_>) -> Vec<Hotkey>;
    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        let _ = ctx;
        Vec::new()
    }
    fn on_input(&mut self, event: &ShellInput, ctx: &mut WidgetContext<'_>) -> ShellAction;
    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>);
}
