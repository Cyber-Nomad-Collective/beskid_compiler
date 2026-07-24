//! Command palette: filter, param entry, and contextual dispatch.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use super::catalog::{CommandItem, CommandKind};
use super::widget::ShellAction;
use crate::tui::overlay_chrome::draw_backdrop;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Browsing,
    Params,
}

#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    pub visible: bool,
    pub mode: PaletteMode,
    pub filter: String,
    pub selected: usize,
    pub items: Vec<CommandItem>,
    pub pending: Option<CommandItem>,
    pub status: Option<String>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: PaletteMode::Browsing,
            filter: String::new(),
            selected: 0,
            items: Vec::new(),
            pending: None,
            status: None,
        }
    }
}

impl CommandPaletteState {
    pub fn open(&mut self, items: Vec<CommandItem>) {
        self.visible = true;
        self.mode = PaletteMode::Browsing;
        self.filter.clear();
        self.selected = 0;
        self.items = items;
        self.pending = None;
        self.status = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.pending = None;
        self.status = None;
    }

    pub fn filtered_items(&self) -> Vec<&CommandItem> {
        let needle = self.filter.to_lowercase();
        if needle.is_empty() {
            return self.items.iter().collect();
        }
        self.items
            .iter()
            .filter(|item| {
                item.name().to_lowercase().contains(&needle)
                    || item.description().to_lowercase().contains(&needle)
                    || item.id().to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PaletteAction {
        if key.kind != KeyEventKind::Press {
            return PaletteAction::None;
        }
        match self.mode {
            PaletteMode::Browsing => self.handle_browse_key(key),
            PaletteMode::Params => self.handle_params_key(key),
        }
    }

    fn handle_browse_key(&mut self, key: KeyEvent) -> PaletteAction {
        let count = self.filtered_items().len();
        match key.code {
            KeyCode::Esc => {
                self.close();
                PaletteAction::Close
            }
            KeyCode::Down => {
                if count > 0 {
                    self.selected = (self.selected + 1).min(count.saturating_sub(1));
                }
                PaletteAction::Redraw
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                PaletteAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(item) = self.filtered_items().get(self.selected).map(|item| (*item).clone()) {
                    return self.select_item(item);
                }
                PaletteAction::Redraw
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
                PaletteAction::Redraw
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.selected = 0;
                PaletteAction::Redraw
            }
            _ => PaletteAction::None,
        }
    }

    fn handle_params_key(&mut self, key: KeyEvent) -> PaletteAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = PaletteMode::Browsing;
                self.pending = None;
                PaletteAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(item) = self.pending.clone() {
                    return PaletteAction::Execute(item, self.filter.clone());
                }
                PaletteAction::Redraw
            }
            KeyCode::Backspace => {
                self.filter.pop();
                PaletteAction::Redraw
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                PaletteAction::Redraw
            }
            _ => PaletteAction::None,
        }
    }

    fn select_item(&mut self, item: CommandItem) -> PaletteAction {
        match &item {
            CommandItem::Workflow(wf) if !wf.args_hint.is_empty() => {
                self.pending = Some(item);
                self.mode = PaletteMode::Params;
                self.filter.clear();
                PaletteAction::Redraw
            }
            _ if item.args_hint().is_some_and(|h| !h.is_empty()) => {
                self.pending = Some(item);
                self.mode = PaletteMode::Params;
                self.filter.clear();
                PaletteAction::Redraw
            }
            _ => PaletteAction::Execute(item, String::new()),
        }
    }

    pub fn render(&self, terminal: Rect, frame: &mut Frame, palette_hint: &str) {
        draw_backdrop(frame, terminal);
        let overlay = centered_rect(60, 70, terminal);
        frame.render_widget(Clear, overlay);
        let [input_area, list_area, hint_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(6), Constraint::Length(2)]).areas(overlay);

        let prompt = match self.mode {
            PaletteMode::Browsing => "Type to filter commands",
            PaletteMode::Params => self.pending.as_ref().and_then(|p| p.args_hint()).unwrap_or("Enter arguments"),
        };
        let input_line = if self.filter.is_empty() {
            Line::from(Span::styled(prompt, Style::default().fg(Color::DarkGray)))
        } else {
            Line::from(self.filter.as_str())
        };
        frame.render_widget(
            Paragraph::new(input_line).block(Block::default().borders(Borders::ALL).title(" Command palette ")),
            input_area,
        );

        let filtered = self.filtered_items();
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let kind = match item.kind() {
                    CommandKind::Workflow => "wf",
                    CommandKind::Contextual => "ctx",
                    CommandKind::Nav => "nav",
                };
                let style = if index == self.selected {
                    Style::default().bg(Color::DarkGray).fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} [{}] {} — {}", item.icon(), kind, item.name(), item.description()))
                    .style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Commands ")),
            list_area,
        );

        let hint = Line::from(vec![
            Span::styled(palette_hint, Style::default().fg(Color::Cyan)),
            Span::raw(" palette · "),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" select · "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" close"),
        ]);
        frame.render_widget(Paragraph::new(hint), hint_area);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaletteAction {
    None,
    Redraw,
    Close,
    Execute(CommandItem, String),
}

pub fn contextual_to_shell_action(item: &CommandItem) -> ShellAction {
    let CommandItem::Contextual(ctx) = item else {
        return ShellAction::None;
    };
    if let Some(widget) = ctx.widget_id {
        return ShellAction::OpenOverlay(widget);
    }
    ShellAction::RunContextual(ctx.id)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
