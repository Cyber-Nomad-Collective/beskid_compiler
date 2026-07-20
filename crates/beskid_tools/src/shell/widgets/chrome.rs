use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Paragraph;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct ChromeWidget;

impl BeskidWidget for ChromeWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.chrome",
            title: "Chrome",
            icon: "—",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, _ctx: &mut WidgetContext<'_>) {
        frame.render_widget(
            Paragraph::new("Ctrl+P palette · q quit").style(Color::DarkGray),
            area,
        );
    }
}
