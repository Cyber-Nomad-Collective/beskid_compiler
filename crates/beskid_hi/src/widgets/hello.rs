use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratkit::services::hotkey_service::Hotkey;

use beskid_tools::shell::{BeskidWidget, ShellAction, ShellInput, WidgetContext, WidgetMeta};

use crate::models::descriptor::{ExtensionWidgetDescriptor, WIDGET_CATALOG};

const DESC: ExtensionWidgetDescriptor = WIDGET_CATALOG[0];

pub struct HelloWidget;

impl HelloWidget {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HelloWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BeskidWidget for HelloWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: DESC.id,
            title: DESC.title,
            icon: DESC.icon,
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
            Paragraph::new("beskid_hi extension widget — shell API surface OK")
                .style(Style::default().fg(Color::Green))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {} ", DESC.id)),
                ),
            area,
        );
    }
}
