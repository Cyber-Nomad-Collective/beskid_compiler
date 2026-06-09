use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratkit::services::hotkey_service::Hotkey;

use crate::shell::catalog::ContextualCommand;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct AnalysisWidget;

impl BeskidWidget for AnalysisWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "analysis.diagnostics",
            title: "Analysis",
            icon: "◇",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        match ctx.scope {
            ShellScope::User => Vec::new(),
            _ => vec![ContextualCommand {
                id: "ctx.analyze",
                name: "Analyze",
                description: "Run semantic analysis in scope",
                icon: "◇",
                args_hint: None,
                widget_id: Some("analysis.diagnostics"),
            }],
        }
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let text = if ctx.shell_state.compile_complete {
            "Analysis complete — use palette to re-run `analyze`."
        } else {
            "Run `analyze` from the command palette to check diagnostics."
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" Analysis ")),
            area,
        );
    }
}
