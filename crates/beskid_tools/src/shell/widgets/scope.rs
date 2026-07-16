use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct ScopeWidget;

impl BeskidWidget for ScopeWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.scope",
            title: "Scope",
            icon: "◎",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let label = ctx.scope.label();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Scope: ", Style::default().fg(Color::DarkGray)),
                Span::styled(label, Style::default().fg(Color::Cyan)),
            ]))
            .block(Block::default().borders(Borders::ALL).title(" Context ")),
            area,
        );
    }
}
