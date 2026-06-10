//! External command dialog — ratkit `Dialog` for nav-defined or arbitrary shell commands.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use ratkit::primitives::dialog::{
    Dialog, DialogAction, DialogBodyRenderer, DialogModalMode, DialogShadow, DialogWidget,
    DialogWrap,
};

use super::catalog::{CliCommandDef, CommandItem};
use super::cli_run::{plan_external_command, CliRunPlan};
use super::scope::ShellScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandDialogAction {
    None,
    Redraw,
    Close,
    Run,
}

struct ArgsBodyState {
    args: String,
    hint: String,
    scope_hint: String,
}

struct ArgsBodyRenderer {
    state: Arc<Mutex<ArgsBodyState>>,
}

impl ArgsBodyRenderer {
    fn new(state: Arc<Mutex<ArgsBodyState>>) -> Self {
        Self { state }
    }
}

impl DialogBodyRenderer for ArgsBodyRenderer {
    fn render_body(&mut self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }
        let state = self.state.lock().expect("command dialog state poisoned");
        let hint_line = Line::from(vec![
            Span::styled(state.hint.as_str(), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(state.scope_hint.as_str(), Style::default().fg(Color::DarkGray)),
        ]);
        buf.set_line(area.x, area.y, &hint_line, area.width);

        let input_y = area.y.saturating_add(2);
        if input_y < area.y + area.height {
            let prompt = if state.args.is_empty() {
                Line::from(Span::styled(
                    "command …",
                    Style::default().fg(Color::DarkGray),
                ))
            } else {
                Line::from(Span::styled(
                    state.args.as_str(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ))
            };
            buf.set_line(area.x, input_y, &prompt, area.width);
        }
    }
}

pub struct CommandDialogOverlay {
    pub visible: bool,
    item: Option<CommandItem>,
    argv_override: Option<Vec<String>>,
    body_state: Arc<Mutex<ArgsBodyState>>,
    dialog: Dialog<'static>,
    title: &'static str,
}

impl Default for CommandDialogOverlay {
    fn default() -> Self {
        Self {
            visible: false,
            item: None,
            argv_override: None,
            body_state: Arc::new(Mutex::new(ArgsBodyState {
                args: String::new(),
                hint: String::new(),
                scope_hint: String::new(),
            })),
            dialog: Dialog::confirm("Run command", "")
                .buttons(vec!["Run", "Cancel"])
                .default_selection(0)
                .overlay(true)
                .modal_mode(DialogModalMode::Blocking)
                .shadow(DialogShadow::Medium)
                .wrap_mode(DialogWrap::WordTrim),
            title: "",
        }
    }
}

impl CommandDialogOverlay {
    /// Open the dialog for an external command defined in navigation (not built-in palette CLI).
    pub fn open_external(&mut self, argv: Vec<String>, scope: &ShellScope) {
        if argv.is_empty() {
            return;
        }
        let name = leak_str(argv.first().cloned().unwrap_or_else(|| "command".into()));
        let cli = CliCommandDef {
            id: "nav.external",
            name,
            description: "External command",
            icon: "▶",
            argv_prefix: &[],
            args_hint: "[args]",
        };
        let item = CommandItem::Cli(cli);
        self.open_inner(item, name, "[args]", argv, scope);
    }

    fn open_inner(
        &mut self,
        item: CommandItem,
        command_name: &str,
        args_hint: &str,
        argv_override: Vec<String>,
        scope: &ShellScope,
    ) {
        let title = leak_str(format!("{command_name}"));
        let hint = args_hint.to_string();
        let scope_hint = scope
            .root_dir()
            .map(|p| format!("scope: {}", p.display()))
            .unwrap_or_else(|| "scope: user".into());
        let prefill = if argv_override.len() > 1 {
            argv_override[1..].join(" ")
        } else {
            String::new()
        };

        {
            let mut state = self.body_state.lock().expect("command dialog state poisoned");
            state.args = prefill;
            state.hint = if hint.is_empty() {
                "Optional arguments".into()
            } else {
                format!("Arguments: {hint}")
            };
            state.scope_hint = scope_hint;
        }

        self.title = title;
        self.item = Some(item);
        self.argv_override = if argv_override.is_empty() {
            None
        } else {
            Some(argv_override)
        };
        self.dialog = Dialog::confirm(title, "")
            .buttons(vec!["Run", "Cancel"])
            .default_selection(0)
            .overlay(true)
            .modal_mode(DialogModalMode::Blocking)
            .shadow(DialogShadow::Medium)
            .wrap_mode(DialogWrap::WordTrim)
            .body_renderer(Box::new(ArgsBodyRenderer::new(self.body_state.clone())));
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.item = None;
        self.argv_override = None;
    }

    pub fn args(&self) -> String {
        self.body_state
            .lock()
            .map(|s| s.args.clone())
            .unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> CommandDialogAction {
        if key.kind != KeyEventKind::Press {
            return CommandDialogAction::None;
        }
        match key.code {
            KeyCode::Esc => {
                self.close();
                CommandDialogAction::Close
            }
            KeyCode::Backspace => {
                if let Ok(mut state) = self.body_state.lock() {
                    state.args.pop();
                }
                CommandDialogAction::Redraw
            }
            KeyCode::Enter => {
                if self.dialog.get_selected_button_text() == Some("Cancel") {
                    self.close();
                    CommandDialogAction::Close
                } else {
                    CommandDialogAction::Run
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Ok(mut state) = self.body_state.lock() {
                    state.args.push(c);
                }
                CommandDialogAction::Redraw
            }
            _ => {
                let result = self.dialog.handle_key_event(key.code);
                if result.consumed {
                    if let Some(DialogAction::Cancel | DialogAction::Close) = result.action {
                        self.close();
                        return CommandDialogAction::Close;
                    }
                    if let Some(DialogAction::Confirm(idx)) = result.action {
                        if self.dialog.buttons.get(idx) == Some(&"Cancel") {
                            self.close();
                            return CommandDialogAction::Close;
                        }
                        return CommandDialogAction::Run;
                    }
                    CommandDialogAction::Redraw
                } else {
                    CommandDialogAction::None
                }
            }
        }
    }

    pub fn take_run_plan(&mut self, _exe: &PathBuf, _scope: &ShellScope) -> Option<CliRunPlan> {
        let params = self.args();
        let argv = self.argv_override.take()?;
        self.item = None;
        let plan = plan_external_command(argv, &params)?;
        self.close();
        Some(plan)
    }

    pub fn render(&mut self, area: Rect, frame: &mut Frame) {
        if !self.visible {
            return;
        }
        frame.render_widget(DialogWidget::new(&mut self.dialog), area);
    }
}

fn leak_str(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
