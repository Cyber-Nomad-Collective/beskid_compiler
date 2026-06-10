use std::cell::RefCell;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use crate::shell::primitives::Hotkey;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::settings::{
    SettingKind, ToolSettingsRegistry, ToolsConfig, get_value, load_config, save_config,
    save_path_for_scope, set_value,
};
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

struct SettingsWidgetState {
    registry: ToolSettingsRegistry,
    config: ToolsConfig,
    saved_config: ToolsConfig,
    active_page: usize,
    focused_field: usize,
    editing: bool,
    edit_buffer: String,
    status: Option<String>,
    scope_key: String,
}

impl Default for SettingsWidgetState {
    fn default() -> Self {
        Self {
            registry: ToolSettingsRegistry::with_builtins(),
            config: ToolsConfig::default(),
            saved_config: ToolsConfig::default(),
            active_page: 0,
            focused_field: 0,
            editing: false,
            edit_buffer: String::new(),
            status: None,
            scope_key: String::new(),
        }
    }
}

impl SettingsWidgetState {
    fn ensure_loaded(&mut self, ctx: &WidgetContext<'_>) {
        let key = ctx.scope.label();
        if self.scope_key == key {
            return;
        }
        self.scope_key = key;
        self.config = load_config(ctx.scope, &self.registry);
        self.saved_config = self.config.clone();
        self.active_page = 0;
        self.focused_field = 0;
        self.editing = false;
        self.edit_buffer.clear();
        self.status = None;
    }

    fn active_page(&self) -> Option<&crate::shell::settings::ToolSettingsPage> {
        self.registry.pages().get(self.active_page)
    }

    fn save(&mut self, ctx: &WidgetContext<'_>) {
        match save_config(ctx.scope, &self.config) {
            Ok(()) => {
                self.saved_config = self.config.clone();
                self.status = Some(format!("Saved to {}", save_path_for_scope(ctx.scope).display()));
            }
            Err(err) => self.status = Some(format!("Save failed: {err}")),
        }
    }

    fn reset(&mut self) {
        self.config = self.saved_config.clone();
        self.editing = false;
        self.edit_buffer.clear();
        self.status = Some("Reset to last saved values".into());
    }
}

pub struct SettingsWidget {
    state: RefCell<SettingsWidgetState>,
}

impl Default for SettingsWidget {
    fn default() -> Self {
        Self {
            state: RefCell::new(SettingsWidgetState::default()),
        }
    }
}

impl BeskidWidget for SettingsWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.settings",
            title: "Settings",
            icon: "⚙",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, event: &ShellInput, ctx: &mut WidgetContext<'_>) -> ShellAction {
        let mut state = self.state.borrow_mut();
        state.ensure_loaded(ctx);
        let ShellInput::Key(key) = event else {
            return ShellAction::None;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            state.save(ctx);
            return ShellAction::Redraw;
        }

        match key.code {
            KeyCode::Char('s') if !state.editing => {
                state.save(ctx);
                ShellAction::Redraw
            }
            KeyCode::Char('r') if !state.editing => {
                state.reset();
                ShellAction::Redraw
            }
            KeyCode::Tab if !state.editing => {
                let page_count = state.registry.pages().len().max(1);
                state.active_page = (state.active_page + 1) % page_count;
                state.focused_field = 0;
                ShellAction::Redraw
            }
            KeyCode::BackTab if !state.editing => {
                let page_count = state.registry.pages().len().max(1);
                state.active_page = (state.active_page + page_count - 1) % page_count;
                state.focused_field = 0;
                ShellAction::Redraw
            }
            KeyCode::Up if !state.editing => {
                state.focused_field = state.focused_field.saturating_sub(1);
                ShellAction::Redraw
            }
            KeyCode::Down if !state.editing => {
                if let Some(page) = state.active_page() {
                    if !page.settings.is_empty() {
                        state.focused_field =
                            (state.focused_field + 1).min(page.settings.len() - 1);
                    }
                }
                ShellAction::Redraw
            }
            KeyCode::Enter => {
                let action = if let Some(page) = state.active_page() {
                    if let Some(desc) = page.settings.get(state.focused_field) {
                        let tool_id = page.tool_id;
                        let key = desc.key;
                        let kind = desc.kind;
                        if kind == SettingKind::Bool {
                            let current = get_value(&state.config, &state.registry, tool_id, key);
                            let next = if current == "true" {
                                "false".into()
                            } else {
                                "true".into()
                            };
                            set_value(&mut state.config, tool_id, key, next);
                            Some(ShellAction::Redraw)
                        } else if !state.editing {
                            state.editing = true;
                            state.edit_buffer =
                                get_value(&state.config, &state.registry, tool_id, key);
                            Some(ShellAction::Redraw)
                        } else {
                            let value = state.edit_buffer.clone();
                            set_value(&mut state.config, tool_id, key, value);
                            state.editing = false;
                            state.edit_buffer.clear();
                            Some(ShellAction::Redraw)
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                return action.unwrap_or(ShellAction::None);
            }
            KeyCode::Esc if state.editing => {
                state.editing = false;
                state.edit_buffer.clear();
                ShellAction::Redraw
            }
            KeyCode::Backspace if state.editing => {
                state.edit_buffer.pop();
                ShellAction::Redraw
            }
            KeyCode::Char(ch) if state.editing => {
                state.edit_buffer.push(ch);
                ShellAction::Redraw
            }
            _ => ShellAction::None,
        }
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let mut state = self.state.borrow_mut();
        state.ensure_loaded(ctx);

        let page = state
            .registry
            .pages()
            .get(state.active_page)
            .or_else(|| state.registry.pages().first());

        let Some(page) = page else {
            frame.render_widget(
                Paragraph::new("No settings pages registered")
                    .block(Block::default().borders(Borders::ALL).title(" Settings ")),
                area,
            );
            return;
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ({})", page.title, page.tool_id),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(Span::styled(
                "Tab — switch tool page   ↑↓ — focus   Enter — edit/toggle   s — save   r — reset",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
        ];

        for (idx, desc) in page.settings.iter().enumerate() {
            let value = if state.editing && idx == state.focused_field {
                state.edit_buffer.clone()
            } else {
                get_value(&state.config, &state.registry, page.tool_id, desc.key)
            };
            let display = match desc.kind {
                SettingKind::Bool => {
                    if value == "true" {
                        "[x]".into()
                    } else {
                        "[ ]".into()
                    }
                }
                SettingKind::U32 | SettingKind::Quoted => value,
            };
            let focused = idx == state.focused_field;
            let prefix = if focused { "> " } else { "  " };
            let style = if focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{}: ", desc.label), style),
                Span::styled(display, style),
            ]));
            if focused {
                lines.push(Line::from(Span::styled(
                    format!("    {}", desc.description),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        if let Some(status) = &state.status {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(Color::Green),
            )));
        }

        let page_tabs: Vec<_> = state
            .registry
            .pages()
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                if idx == state.active_page {
                    format!("[{}]", p.title)
                } else {
                    p.title.to_string()
                }
            })
            .collect();

        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Settings — {} ", page_tabs.join(" | "))),
            ),
            area,
        );
    }
}
