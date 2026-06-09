//! Shell host: `beskid hi` entry and ratkit runner.

use std::env;
use std::io::{self, IsTerminal, stderr};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};
use ratkit::{CoordinatorAction, CoordinatorApp, CoordinatorEvent, LayoutResult, RunnerConfig};

use super::chrome::ShellChrome;
use super::context::WidgetContext;
use super::hotkeys::ShellHotkeys;
use super::layout::{self, HiLayoutState, LayoutEditCommand};
use super::palette::{self, CommandPaletteState, PaletteAction};
use super::registry::WidgetRegistry;
use super::scope::ShellScope;
use super::scope_picker::{ScopePickerAction, ScopePickerMode, ScopePickerOverlay, resolve_picked_scope};
use super::widget::ShellAction;
use super::widgets::{self, open_pckg, open_tests};
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::shell::effects::{apply_effects, drain_pending_work};
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::pane_state::ShellMode;
use crate::tui::shell::runtime::RuntimeOp;
use crate::tui::shell::state::ShellState;
use crate::tui::views;

pub type WidgetRegistrar = fn(&mut WidgetRegistry);

pub struct ShellHost;

impl ShellHost {
    pub fn interactive_available(plain: bool) -> bool {
        !plain && !no_color_requested() && stderr().is_terminal()
    }

    pub fn run_hi_blocking(
        scope: ShellScope,
        plain: bool,
        extra_registrars: &[WidgetRegistrar],
    ) -> io::Result<()> {
        if !Self::interactive_available(plain) {
            eprintln!("beskid hi: terminal UI requires an interactive stderr TTY");
            return Ok(());
        }
        let layout = layout::load_for_scope(&scope).map_err(io::Error::other)?;
        let mut registry = WidgetRegistry::new();
        widgets::register_builtins(&mut registry);
        for register in extra_registrars {
            register(&mut registry);
        }
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("beskid"));
        let app = HiShellApp::new(scope, layout, registry, exe);
        ratkit::run_with_diagnostics(
            app,
            RunnerConfig {
                tick_rate: std::time::Duration::from_millis(80),
                ..RunnerConfig::default()
            },
        )
    }
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some()
}

pub struct HiShellApp {
    pub scope: ShellScope,
    pub layout: HiLayoutState,
    pub registry: WidgetRegistry,
    pub shell_state: ShellState,
    pub palette: CommandPaletteState,
    pub chrome: ShellChrome,
    pub hotkeys: ShellHotkeys,
    pub focused_widget: String,
    pub beskid_exe: PathBuf,
    pub quit_requested: bool,
    pub scope_picker: Option<ScopePickerOverlay>,
    msg_tx: Sender<RuntimeOp>,
    msg_rx: Receiver<RuntimeOp>,
}

impl HiShellApp {
    pub fn new(
        scope: ShellScope,
        layout: HiLayoutState,
        registry: WidgetRegistry,
        beskid_exe: PathBuf,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        let mut shell_state = ShellState::default();
        shell_state.shell_mode = ShellMode::Hi;
        let focused = layout
            .doc
            .nodes
            .values()
            .find_map(|n| n.widget.clone())
            .unwrap_or_else(|| "hi.welcome".into());
        Self {
            scope,
            layout,
            registry,
            shell_state,
            palette: CommandPaletteState::default(),
            chrome: ShellChrome::default(),
            hotkeys: ShellHotkeys::default(),
            focused_widget: focused,
            beskid_exe,
            quit_requested: false,
            scope_picker: None,
            msg_tx,
            msg_rx,
        }
    }

    fn widget_context(&mut self) -> WidgetContext<'_> {
        WidgetContext::new(
            &self.scope,
            &self.layout.doc,
            &mut self.shell_state,
            &mut self.palette,
            &self.focused_widget,
            &self.beskid_exe,
        )
    }

    fn drain_messages(&mut self) {
        while let Ok(op) = self.msg_rx.try_recv() {
            if let RuntimeOp::Update(msg) = op {
                let effects = views::update(&msg, &mut self.shell_state);
                apply_effects(effects, &self.msg_tx, &mut self.shell_state);
            }
        }
        drain_pending_work(&self.msg_tx, &mut self.shell_state);
        let _ = self.layout.maybe_autosave(&self.scope);
    }

    fn open_palette(&mut self) {
        let items = self
            .registry
            .palette_commands(&self.scope, self.layout.editor.active);
        self.palette.open(items);
    }

    fn handle_shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::Quit => self.quit_requested = true,
            ShellAction::Redraw => {}
            ShellAction::OpenPalette => self.open_palette(),
            ShellAction::OpenOverlay(widget_id) => self.open_overlay(widget_id),
            ShellAction::RunContextual(id) => self.run_contextual(id),
            ShellAction::None => {}
        }
    }

    fn open_overlay(&mut self, widget_id: &str) {
        let mut ctx = self.widget_context();
        match widget_id {
            "pckg.browser" => open_pckg(&mut ctx),
            "tests.runner" => open_tests(&mut ctx),
            "templates.picker" => {
                ctx.shell_state
                    .set_overlay_visible(OverlayKind::Templates, true);
                ctx.shell_state.focus_overlay(OverlayKind::Templates);
            }
            "graph.deps" => {
                let _ = palette::execute_cli_command(
                    &self.beskid_exe,
                    &super::catalog::CommandItem::Cli(super::catalog::CliCommandDef {
                        id: "graph",
                        name: "graph",
                        description: "graph",
                        icon: "◎",
                        argv_prefix: &["graph", "--tui"],
                        args_hint: "",
                    }),
                    "",
                    &self.scope,
                );
            }
            _ => {}
        }
    }

    fn run_contextual(&mut self, id: &str) {
        match id {
            "ctx.palette" => self.open_palette(),
            "ctx.layout_edit" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::ToggleEdit, &self.scope, None);
            }
            "ctx.open_workspace" => {
                self.scope_picker = ScopePickerOverlay::open(ScopePickerMode::Workspace).ok();
            }
            "ctx.open_project" => {
                self.scope_picker = ScopePickerOverlay::open(ScopePickerMode::Project).ok();
            }
            "layout.focus_next" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::FocusNext,
                    &self.scope,
                    None,
                );
            }
            "layout.focus_prev" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::FocusPrev,
                    &self.scope,
                    None,
                );
            }
            "layout.add" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::AddPanel,
                    &self.scope,
                    None,
                );
            }
            "layout.remove" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::RemovePanel,
                    &self.scope,
                    None,
                );
            }
            "layout.wrap_col" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::WrapCol,
                    &self.scope,
                    None,
                );
            }
            "layout.wrap_row" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::WrapRow,
                    &self.scope,
                    None,
                );
            }
            "layout.tabs" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::ConvertTabs,
                    &self.scope,
                    None,
                );
            }
            "layout.stack" => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::ConvertStack,
                    &self.scope,
                    None,
                );
            }
            "layout.save" => {
                let _ = self.layout.apply_command(LayoutEditCommand::Save, &self.scope, None);
            }
            "layout.reset" => {
                let _ = self.layout.apply_command(LayoutEditCommand::Reset, &self.scope, None);
            }
            _ => {}
        }
    }

    fn handle_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::None | PaletteAction::Redraw => {}
            PaletteAction::Close => {}
            PaletteAction::Execute(item, params) => {
                self.palette.close();
                match item.kind() {
                    super::catalog::CommandKind::Cli => {
                        let _ = palette::execute_cli_command(
                            &self.beskid_exe,
                            &item,
                            &params,
                            &self.scope,
                        );
                    }
                    super::catalog::CommandKind::Contextual => {
                        if item.id().starts_with("layout.") {
                            let widget = if item.id() == "layout.add" || item.id() == "layout.set_widget" {
                                let w = params.trim();
                                if w.is_empty() { None } else { Some(w) }
                            } else {
                                None
                            };
                            if let Some(w) = widget {
                                let cmd = if item.id() == "layout.add" {
                                    LayoutEditCommand::AddPanel
                                } else {
                                    LayoutEditCommand::SetWidget
                                };
                                let _ = self.layout.apply_command(cmd, &self.scope, Some(w));
                            } else {
                                self.run_contextual(item.id());
                            }
                        } else {
                            self.handle_shell_action(palette::contextual_to_shell_action(&item));
                        }
                    }
                }
            }
        }
    }

    fn reload_scope(&mut self, scope: ShellScope) {
        if let Ok(layout) = layout::load_for_scope(&scope) {
            self.scope = scope;
            self.layout = layout;
            self.focused_widget = self
                .layout
                .doc
                .nodes
                .values()
                .find_map(|n| n.widget.clone())
                .unwrap_or_else(|| "hi.welcome".into());
        }
    }
}

impl CoordinatorApp for HiShellApp {
    fn on_event(&mut self, event: CoordinatorEvent) -> LayoutResult<CoordinatorAction> {
        self.drain_messages();

        if let Some(picker) = self.scope_picker.as_mut() {
            if let CoordinatorEvent::Keyboard(keyboard) = &event {
                let key = KeyEvent {
                    code: keyboard.key_code,
                    modifiers: keyboard.modifiers,
                    kind: keyboard.kind,
                    state: crossterm::event::KeyEventState::empty(),
                };
                match picker.handle_key(key) {
                    ScopePickerAction::Close => self.scope_picker = None,
                    ScopePickerAction::Redraw => {}
                    ScopePickerAction::Selected(path) => {
                        let scope = resolve_picked_scope(&path);
                        self.reload_scope(scope);
                        self.scope_picker = None;
                    }
                }
                return Ok(CoordinatorAction::Redraw);
            }
            return Ok(CoordinatorAction::Redraw);
        }

        if self.palette.visible {
            if let CoordinatorEvent::Keyboard(keyboard) = &event {
                let key = KeyEvent {
                    code: keyboard.key_code,
                    modifiers: keyboard.modifiers,
                    kind: keyboard.kind,
                    state: crossterm::event::KeyEventState::empty(),
                };
                let action = self.palette.handle_key(key);
                self.handle_palette_action(action);
                return Ok(CoordinatorAction::Redraw);
            }
            return Ok(CoordinatorAction::Redraw);
        }

        if self.layout.editor.active {
            if let CoordinatorEvent::Keyboard(keyboard) = &event {
                if keyboard.is_key_down() {
                    let key = KeyEvent {
                        code: keyboard.key_code,
                        modifiers: keyboard.modifiers,
                        kind: keyboard.kind,
                        state: crossterm::event::KeyEventState::empty(),
                    };
                    match key.code {
                        KeyCode::Esc => {
                            let _ = self.layout.apply_command(
                                LayoutEditCommand::ToggleEdit,
                                &self.scope,
                                None,
                            );
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            let _ = self.layout.apply_command(
                                LayoutEditCommand::ResizePlus,
                                &self.scope,
                                None,
                            );
                        }
                        KeyCode::Char('-') | KeyCode::Char('_') => {
                            let _ = self.layout.apply_command(
                                LayoutEditCommand::ResizeMinus,
                                &self.scope,
                                None,
                            );
                        }
                        _ => {}
                    }
                    return Ok(CoordinatorAction::Redraw);
                }
            }
        }

        match event {
            CoordinatorEvent::Keyboard(keyboard) if keyboard.is_key_down() => {
                let key = KeyEvent {
                    code: keyboard.key_code,
                    modifiers: keyboard.modifiers,
                    kind: keyboard.kind,
                    state: crossterm::event::KeyEventState::empty(),
                };
                if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p'))
                    || key.code == KeyCode::Char(':')
                {
                    self.open_palette();
                    return Ok(CoordinatorAction::Redraw);
                }
                if key.code == KeyCode::Char('?') {
                    self.chrome.show_help = !self.chrome.show_help;
                    return Ok(CoordinatorAction::Redraw);
                }
                if key.code == KeyCode::Char('q') {
                    self.quit_requested = true;
                    return Ok(CoordinatorAction::Quit);
                }
                let input = InputEvent::Key(key);
                let result = views::on_input(&input, &mut self.shell_state);
                match result {
                    InputResult::Quit => {
                        self.quit_requested = true;
                        Ok(CoordinatorAction::Quit)
                    }
                    InputResult::CloseOverlay => {
                        self.shell_state.close_focused_overlay();
                        Ok(CoordinatorAction::Redraw)
                    }
                    _ => Ok(CoordinatorAction::Redraw),
                }
            }
            CoordinatorEvent::Tick(_) => {
                self.drain_messages();
                Ok(CoordinatorAction::Redraw)
            }
            CoordinatorEvent::Resize(_) => Ok(CoordinatorAction::Redraw),
            _ => Ok(CoordinatorAction::Continue),
        }
    }

    fn on_draw(&mut self, frame: &mut Frame) {
        self.drain_messages();
        let area = frame.area();
        if let Some(kind) = self.layout.runtime.focused_kind() {
            self.focused_widget = kind.to_string();
        }
        let edit_active = self.layout.editor.active;
        let highlight = self.focused_widget.clone();

        let resolved = match super::layout::resolve::resolve_panels(&mut self.layout.runtime, area)
        {
            Ok(r) => r,
            Err(_) => return,
        };

        for entry in resolved.frame.panels() {
            let rect = entry.rect;
            let widget_id = entry.kind.to_string();
            let mut ctx = WidgetContext::new(
                &self.scope,
                &self.layout.doc,
                &mut self.shell_state,
                &mut self.palette,
                &self.focused_widget,
                &self.beskid_exe,
            );
            if let Some(widget) = self.registry.get(&widget_id) {
                widget.render(rect, frame, &mut ctx);
            }
            if edit_active && highlight == entry.kind.as_ref() as &str {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                    rect,
                );
            }
        }

        self.chrome.render_footer(
            resolved.chrome_area,
            frame,
            &self.hotkeys,
            Some(self.focused_widget.as_str()),
        );

        if self.shell_state.overlay_visible(OverlayKind::Tests) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_TESTS, area);
            self.shell_state.layout_rects.tests_overlay = Some(overlay);
            crate::tui::screens::tests_overlay::render(overlay, frame, &mut self.shell_state);
        }
        if self.shell_state.overlay_visible(OverlayKind::Pckg) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_PCKG, area);
            self.shell_state.layout_rects.pckg_overlay = Some(overlay);
            crate::tui::screens::pckg_overlay::render(overlay, frame, &mut self.shell_state);
        }
        if self.shell_state.overlay_visible(OverlayKind::Templates) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_TEMPLATES, area);
            self.shell_state.layout_rects.templates_overlay = Some(overlay);
            crate::tui::screens::templates_overlay::render(overlay, frame, &mut self.shell_state);
        }

        if self.chrome.show_help {
            let help_area =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_TESTS, area);
            self.chrome.render_help_overlay(
                help_area,
                frame,
                &self.hotkeys.footer_items(Some(&self.focused_widget)),
            );
        }

        if let Some(picker) = &self.scope_picker {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_PCKG, area);
            picker.render(overlay, frame);
        }

        if self.palette.visible {
            self.palette.render(area, frame);
        }
    }
}
