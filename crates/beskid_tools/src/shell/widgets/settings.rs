use std::cell::RefCell;

use crate::shell::primitives::Hotkey;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::key_bindings::{
    BINDABLE_ACTIONS, ShortcutBindings, chord_from_key, display_chord,
};
use crate::shell::settings::{
    SettingKind, ToolSettingsRegistry, ToolsConfig, get_value, load_config, save_config,
    save_path_for_scope, set_value,
};
use crate::shell::shortcut_clicks::ShortcutClickAction;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

struct SettingsWidgetState {
    registry: ToolSettingsRegistry,
    config: ToolsConfig,
    saved_config: ToolsConfig,
    bindings: ShortcutBindings,
    active_page: usize,
    focused_field: usize,
    editing: bool,
    edit_buffer: String,
    rebinding_action: Option<usize>,
    status: Option<String>,
    scope_key: String,
}

impl Default for SettingsWidgetState {
    fn default() -> Self {
        Self {
            registry: ToolSettingsRegistry::with_builtins(),
            config: ToolsConfig::default(),
            saved_config: ToolsConfig::default(),
            bindings: ShortcutBindings::platform_defaults(),
            active_page: 0,
            focused_field: 0,
            editing: false,
            edit_buffer: String::new(),
            rebinding_action: None,
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
        self.bindings = ShortcutBindings::load(&self.config, &self.registry);
        self.active_page = 0;
        self.focused_field = 0;
        self.editing = false;
        self.edit_buffer.clear();
        self.rebinding_action = None;
        self.status = None;
    }

    fn sync_bindings_to_host(&self, ctx: &mut WidgetContext<'_>) {
        *ctx.key_bindings = self.bindings.clone();
    }

    fn active_page(&self) -> Option<&crate::shell::settings::ToolSettingsPage> {
        self.registry.pages().get(self.active_page)
    }

    fn is_shortcuts_page(&self) -> bool {
        self.active_page()
            .is_some_and(|page| page.tool_id == "shortcuts")
    }

    fn field_count(&self) -> usize {
        if self.is_shortcuts_page() {
            BINDABLE_ACTIONS.len()
        } else {
            self.active_page().map(|p| p.settings.len()).unwrap_or(0)
        }
    }

    fn save(&mut self, ctx: &mut WidgetContext<'_>) {
        self.bindings.save(&mut self.config);
        match save_config(ctx.scope, &self.config) {
            Ok(()) => {
                self.saved_config = self.config.clone();
                self.sync_bindings_to_host(ctx);
                self.status = Some(format!(
                    "Saved to {}",
                    save_path_for_scope(ctx.scope).display()
                ));
            }
            Err(err) => self.status = Some(format!("Save failed: {err}")),
        }
    }

    fn reset(&mut self, ctx: &mut WidgetContext<'_>) {
        self.config = self.saved_config.clone();
        self.bindings = ShortcutBindings::load(&self.config, &self.registry);
        self.editing = false;
        self.edit_buffer.clear();
        self.rebinding_action = None;
        self.sync_bindings_to_host(ctx);
        self.status = Some("Reset to last saved values".into());
    }

    fn reset_bindings_defaults(&mut self, ctx: &mut WidgetContext<'_>) {
        self.bindings.reset_to_defaults();
        self.bindings.save(&mut self.config);
        self.sync_bindings_to_host(ctx);
        self.rebinding_action = None;
        self.status = Some("Shortcuts reset to platform defaults (save to persist)".into());
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

        if let Some(action_idx) = state.rebinding_action {
            match key.code {
                KeyCode::Esc => {
                    state.rebinding_action = None;
                    state.status = None;
                    return ShellAction::Redraw;
                }
                _ => {
                    let chord = chord_from_key(key);
                    let action = BINDABLE_ACTIONS[action_idx];
                    state.bindings.set_chord(action.id, chord);
                    state.rebinding_action = None;
                    state.sync_bindings_to_host(ctx);
                    state.status = Some(format!(
                        "Bound {} to {}",
                        action.label,
                        display_chord(chord)
                    ));
                    return ShellAction::Redraw;
                }
            }
        }

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
                if state.is_shortcuts_page() {
                    state.reset_bindings_defaults(ctx);
                } else {
                    state.reset(ctx);
                }
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
                let count = state.field_count();
                if count > 0 {
                    state.focused_field = (state.focused_field + 1).min(count - 1);
                }
                ShellAction::Redraw
            }
            KeyCode::Enter => {
                if state.is_shortcuts_page() {
                    if state.focused_field < BINDABLE_ACTIONS.len() {
                        state.rebinding_action = Some(state.focused_field);
                        let label = BINDABLE_ACTIONS[state.focused_field].label;
                        state.status = Some(format!("Press a key to bind {label} (Esc cancel)"));
                        return ShellAction::Redraw;
                    }
                    return ShellAction::None;
                }
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
                action.unwrap_or(ShellAction::None)
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
        if let Some(index) = ctx.pending_shortcut_rebind.take()
            && index < BINDABLE_ACTIONS.len()
        {
            state.rebinding_action = Some(index);
            state.focused_field = index;
            let label = BINDABLE_ACTIONS[index].label;
            state.status = Some(format!("Press a key to bind {label} (Esc cancel)"));
        }

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

        let mut lines = vec![Line::from(vec![
            Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({})", page.title, page.tool_id),
                Style::default().fg(Color::Cyan),
            ),
        ])];

        if state.is_shortcuts_page() {
            lines.push(Line::from(Span::styled(
                "Tab — switch page   ↑↓ — select   Enter/click — rebind   s — save   r — reset defaults",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Tab — switch tool page   ↑↓ — focus   Enter — edit/toggle   s — save   r — reset",
                Style::default().fg(Color::DarkGray),
            )));
        }
        lines.push(Line::from(""));

        let mut shortcut_click_rows: Vec<(u16, usize)> = Vec::new();
        let mut line_row: u16 = 0;

        if state.is_shortcuts_page() {
            for (idx, action) in BINDABLE_ACTIONS.iter().enumerate() {
                let focused = idx == state.focused_field;
                shortcut_click_rows.push((line_row, idx));
                let prefix = if focused { "> " } else { "  " };
                let style = if focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().add_modifier(Modifier::UNDERLINED)
                };
                let binding = state.bindings.label_for(action.id);
                lines.push(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{:<22}", action.label), style),
                    Span::styled(binding, style),
                ]));
                line_row += 1;
                if focused {
                    lines.push(Line::from(Span::styled(
                        format!("    {}", action.description),
                        Style::default().fg(Color::DarkGray),
                    )));
                    line_row += 1;
                }
            }
        } else {
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

        if state.is_shortcuts_page() {
            let content = Rect {
                x: area.x.saturating_add(1),
                y: area.y.saturating_add(1),
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };
            for (row, index) in shortcut_click_rows {
                ctx.shortcut_clicks.add_row(
                    content,
                    row,
                    ShortcutClickAction::RebindShortcut(index),
                );
            }
        }
    }
}
