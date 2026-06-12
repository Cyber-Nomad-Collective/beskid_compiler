//! Shell host: `beskid hi` entry and tuirealm runtime.

use std::env;
use std::io::{self, IsTerminal, stderr};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::chrome::ShellChrome;
use super::command_dialog::{CommandDialogAction, CommandDialogOverlay};
use super::context::WidgetContext;
use super::control_mode::HiControlMode;
use super::hotkeys::ShellHotkeys;
use super::nav::NavAction;
use super::layout::{
    self, HiLayoutState, LayoutEditCommand, LayoutEditorOverlay, LayoutOverlayAction,
    switch_page, template_by_id,
};
use super::nav::{NavRegistrar, NavRegistry};
use super::top_menu::{ShellTopMenu, TopMenuAction};
use super::cli_run::{plan_cli_command, CliRunPlan};
use super::hi_compile::{self, HiCompileJob, HiCompileRegistrar, HiCompileRequest};
use super::palette::{self, CommandPaletteState, PaletteAction};
use super::key_bindings::ShortcutBindings;
use super::layers::ShellLayer;
use super::registry::WidgetRegistry;
use super::scope::ShellScope;
use super::scope_picker::{ScopePickerAction, ScopePickerMode, ScopePickerOverlay, resolve_picked_scope};
use super::settings::{ToolSettingsRegistrar, ToolSettingsRegistry, load_config};
use super::widget::{BeskidWidget, ShellAction};
use super::widgets::{self, open_analysis, open_compile_debug, open_pckg, open_tests};
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent, mouse_is_click, mouse_is_inside};
use crate::tui::shell::effects::{apply_effects, drain_pending_work};
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::pane_state::ShellMode;
use crate::pipeline::tui::widgets::init_session_logger;
use crate::tui::message::ShellMessage;
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
        widget_registrars: &[WidgetRegistrar],
        nav_registrars: &[NavRegistrar],
        settings_registrars: &[ToolSettingsRegistrar],
        compile_registrar: Option<HiCompileRegistrar>,
    ) -> io::Result<()> {
        if !Self::interactive_available(plain) {
            eprintln!("beskid hi: terminal UI requires an interactive stderr TTY");
            return Ok(());
        }
        let layout = layout::load_for_scope(&scope).map_err(io::Error::other)?;
        let mut registry = WidgetRegistry::new();
        widgets::register_builtins(&mut registry);
        for register in widget_registrars {
            register(&mut registry);
        }
        let mut nav = NavRegistry::new();
        nav.merge_pages(&layout.pages);
        for register in nav_registrars {
            register(&mut nav);
        }
        let mut settings = ToolSettingsRegistry::with_builtins();
        for register in settings_registrars {
            register(&mut settings);
        }
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("beskid"));
        let app = HiShellApp::new(scope, layout, registry, nav, settings, exe, compile_registrar);
        crate::tui::realm::run_hi(app)
    }
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some()
}

pub struct HiShellApp {
    pub scope: ShellScope,
    pub layout: HiLayoutState,
    pub registry: WidgetRegistry,
    pub nav: NavRegistry,
    pub settings: ToolSettingsRegistry,
    pub shell_state: ShellState,
    pub palette: CommandPaletteState,
    pub chrome: ShellChrome,
    pub hotkeys: ShellHotkeys,
    pub focused_widget: String,
    pub beskid_exe: PathBuf,
    pub quit_requested: bool,
    pub scope_picker: Option<ScopePickerOverlay>,
    pub top_menu: ShellTopMenu,
    pub layout_editor: LayoutEditorOverlay,
    pub command_dialog: CommandDialogOverlay,
    key_bindings: ShortcutBindings,
    pending_cli: Option<CliRunPlan>,
    pending_compile: Option<HiCompileJob>,
    compile_registrar: Option<HiCompileRegistrar>,
    pinned_header: Rect,
    frame_area: Rect,
    msg_tx: Sender<RuntimeOp>,
    msg_rx: Receiver<RuntimeOp>,
}

impl HiShellApp {
    pub fn new(
        scope: ShellScope,
        mut layout: HiLayoutState,
        registry: WidgetRegistry,
        nav: NavRegistry,
        settings: ToolSettingsRegistry,
        beskid_exe: PathBuf,
        compile_registrar: Option<HiCompileRegistrar>,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        let mut scope = scope;
        if scope.is_user() {
            if let Ok(cwd) = env::current_dir() {
                scope = ShellScope::resolve_cwd(&cwd);
            }
        }
        let shell_state = ShellState {
            shell_mode: ShellMode::Hi,
            compile_complete: true,
            ..Default::default()
        };
        let active_page = layout.active_page_id.clone();
        let _ = switch_page(&mut layout, &active_page);
        let focused = layout
            .doc
            .nodes
            .values()
            .find_map(|n| n.widget.clone())
            .unwrap_or_else(|| "hi.welcome".into());
        let hotkeys = ShellHotkeys::default();
        let config = load_config(&scope, &settings);
        let key_bindings = ShortcutBindings::load(&config, &settings);
        let mut top_menu = ShellTopMenu::new();
        top_menu.rebuild(&nav, &layout.pages);
        Self {
            scope,
            layout,
            registry,
            nav,
            settings,
            shell_state,
            palette: CommandPaletteState::default(),
            chrome: ShellChrome::default(),
            hotkeys,
            focused_widget: focused,
            beskid_exe,
            quit_requested: false,
            scope_picker: None,
            top_menu,
            layout_editor: LayoutEditorOverlay::default(),
            command_dialog: CommandDialogOverlay::default(),
            key_bindings,
            pending_cli: None,
            pending_compile: None,
            compile_registrar,
            pinned_header: Rect::default(),
            frame_area: Rect::default(),
            msg_tx,
            msg_rx,
        }
    }

    fn last_frame_area(&self) -> Rect {
        self.frame_area
    }

    pub(crate) fn set_frame_area(&mut self, area: Rect) {
        self.frame_area = area;
    }

    fn control_mode(&self) -> HiControlMode {
        if self.command_dialog.visible {
            HiControlMode::CommandDialog
        } else if self.palette.visible {
            HiControlMode::Palette
        } else if self.top_menu.is_active() {
            HiControlMode::TopMenu
        } else if self.layout.editor.active {
            HiControlMode::LayoutEdit
        } else {
            HiControlMode::Normal
        }
    }

    fn sync_hotkey_scope(&mut self) {
        self.hotkeys.set_control_mode(self.control_mode());
    }

    fn toggle_layout_drawer(&mut self) {
        self.layout.editor.drawer_visible = !self.layout.editor.drawer_visible;
        if self.layout.editor.drawer_visible {
            self.layout.editor.overlay_tab = super::layout::LayoutOverlayTab::Widgets;
            self.layout_editor.refresh_saved_boards(&self.scope);
        }
    }

    fn layout_drawer_rect(&self, area: Rect) -> Rect {
        let width = (area.width as u32 * 40 / 100).max(20) as u16;
        Rect {
            x: area.x + area.width.saturating_sub(width),
            y: area.y,
            width: width.min(area.width),
            height: area.height,
        }
    }

    fn handle_mouse(&mut self, mouse: &MouseEvent, area: Rect) -> ShellOutcome {
        if !mouse_is_click(mouse) {
            return ShellOutcome::Continue;
        }

        if self.top_menu.dropdown_open() {
            let action = self.top_menu.handle_mouse(mouse.column, mouse.row);
            if action != TopMenuAction::None {
                self.handle_top_menu_action(action);
            }
            return ShellOutcome::Redraw;
        }

        if self.command_dialog.visible || self.palette.visible {
            return ShellOutcome::Redraw;
        }

        if self.scope_picker.is_some() {
            return ShellOutcome::Redraw;
        }

        if mouse_is_click(mouse)
            && (mouse_is_inside(mouse, self.pinned_header) || self.top_menu.is_active())
        {
            let action = self.top_menu.handle_mouse(mouse.column, mouse.row);
            if action != TopMenuAction::None {
                self.handle_top_menu_action(action);
                return ShellOutcome::Redraw;
            }
        }

        if self.layout.editor.active {
            if self.layout.editor.drawer_visible {
                let drawer = self.layout_drawer_rect(area);
                if mouse_is_inside(mouse, drawer) {
                    return ShellOutcome::Redraw;
                }
            }
            let panel_id =
                super::layout::resolve::resolve_panels(&mut self.layout.runtime, area)
                    .ok()
                    .and_then(|resolved| {
                        super::layout::resolve::panel_id_at_terminal(
                            &resolved.frame,
                            resolved.main_area,
                            mouse.column,
                            mouse.row,
                        )
                    });
            if let Some(pid) = panel_id {
                self.layout.runtime.focus(pid);
                return ShellOutcome::Redraw;
            }
        }
        if self.shell_state.focus.is_overlay() || self.shell_state.any_overlay_visible() {
            let result = views::on_input(&InputEvent::Mouse(*mouse), &mut self.shell_state);
            return match result {
                InputResult::Quit => {
                    self.quit_requested = true;
                    ShellOutcome::Quit
                }
                InputResult::CloseOverlay => {
                    self.shell_state.close_focused_overlay();
                    ShellOutcome::Redraw
                }
                _ => ShellOutcome::Redraw,
            };
        }
        ShellOutcome::Continue
    }

    fn widget_context(&mut self) -> WidgetContext<'_> {
        WidgetContext::new(
            &self.scope,
            &self.layout.doc,
            &mut self.shell_state,
            &mut self.palette,
            &self.focused_widget,
            &self.beskid_exe,
            &mut self.key_bindings,
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
        let items = self.registry.palette_commands(
            &self.scope,
            self.layout.editor.active,
            &self.nav,
            &self.layout.pages,
        );
        self.palette.open(items);
        self.sync_hotkey_scope();
    }

    fn sync_focus_after_page_switch(&mut self) {
        if let Some(kind) = self.layout.runtime.focused_kind() {
            self.focused_widget = kind.to_string();
            return;
        }
        for widget in ["hi.welcome", "graph.deps", "compile.debugger", "analysis.diagnostics"] {
            if super::layout::resolve::focus_panel_by_kind(&mut self.layout.runtime, widget) {
                self.focused_widget = widget.to_string();
                return;
            }
        }
    }

    fn dispatch_nav_action(&mut self, action: NavAction) {
        match action {
            NavAction::Page(page_id) => {
                if switch_page(&mut self.layout, &page_id).is_ok() {
                    self.sync_focus_after_page_switch();
                }
                self.layout_editor.refresh_saved_boards(&self.scope);
            }
            NavAction::Overlay(widget_id) | NavAction::Widget(widget_id) => {
                self.open_overlay(&widget_id);
            }
            NavAction::Cli(argv) => {
                self.command_dialog.open_external(argv, &self.scope);
                self.sync_hotkey_scope();
            }
            NavAction::Group => {}
        }
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
                ctx.shell_state.set_overlay_visible(OverlayKind::Graph, true);
                ctx.shell_state.focus_overlay(OverlayKind::Graph);
            }
            "compile.debugger" => open_compile_debug(&mut ctx),
            "analysis.diagnostics" => open_analysis(&mut ctx),
            "shell.settings" => {
                ctx.shell_state
                    .set_overlay_visible(OverlayKind::Settings, true);
                ctx.shell_state.focus_overlay(OverlayKind::Settings);
            }
            _ => {}
        }
    }

    fn handle_top_menu_action(&mut self, action: TopMenuAction) {
        match action {
            TopMenuAction::None | TopMenuAction::Redraw => {}
            TopMenuAction::SwitchPage(page_id) => {
                self.dispatch_nav_action(NavAction::Page(page_id));
            }
            TopMenuAction::OpenOverlay(widget_id) => self.open_overlay(&widget_id),
            TopMenuAction::RunCli(argv) => {
                self.dispatch_nav_action(NavAction::Cli(argv));
            }
        }
        self.sync_hotkey_scope();
    }

    fn handle_layout_overlay_action(&mut self, action: LayoutOverlayAction) {
        match action {
            LayoutOverlayAction::None | LayoutOverlayAction::Redraw => {}
            LayoutOverlayAction::ApplyTemplate(id) => {
                if let Some(t) = template_by_id(id) {
                    let preserved = self.layout.doc.nodes.clone();
                    let active_root = self.layout.doc.root.clone();
                    (t.apply)(&mut self.layout.doc);
                    for (id, node) in preserved {
                        if id != active_root && !self.layout.doc.nodes.contains_key(&id) {
                            self.layout.doc.nodes.insert(id, node);
                        }
                    }
                    self.layout.doc.root = active_root;
                    let _ = self.layout.rebuild_runtime();
                    self.layout.mark_dirty();
                }
            }
            LayoutOverlayAction::SetWidget(widget_id) => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::SetWidget,
                    &self.scope,
                    Some(widget_id),
                );
            }
            LayoutOverlayAction::AddWidget(widget_id) => {
                let _ = self.layout.apply_command(
                    LayoutEditCommand::AddPanel,
                    &self.scope,
                    Some(widget_id),
                );
            }
            LayoutOverlayAction::LoadBoard(path) => {
                if let Ok((doc, runtime)) = layout::load::load_from_source(
                    &std::fs::read_to_string(&path).unwrap_or_default(),
                ) {
                    self.layout.doc = doc;
                    self.layout.runtime = runtime;
                    self.layout.mark_dirty();
                }
            }
            LayoutOverlayAction::FocusNode(node_id) => {
                if let Some(node) = self.layout.doc.nodes.get(&node_id)
                    && let Some(widget) = &node.widget {
                        let _ = super::layout::resolve::focus_panel_by_kind(
                            &mut self.layout.runtime,
                            widget,
                        );
                    }
            }
        }
    }

    fn run_contextual(&mut self, id: &str) {
        match id {
            "ctx.palette" => self.open_palette(),
            "ctx.layout_edit" => {
                let was_active = self.layout.editor.active;
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::ToggleEdit, &self.scope, None);
                if self.layout.editor.active && !was_active {
                    self.layout_editor.refresh_saved_boards(&self.scope);
                }
                self.sync_hotkey_scope();
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
                self.nav.merge_pages(&self.layout.pages);
            }
            _ => {}
        }
    }

    fn queue_cli(&mut self, item: &super::catalog::CommandItem, params: &str) {
        if let Some(plan) = plan_cli_command(&self.beskid_exe, item, params, &self.scope) {
            self.pending_cli = Some(plan);
        }
    }

    fn try_refresh_scope(&mut self) {
        let Ok(cwd) = env::current_dir() else {
            return;
        };
        let detected = ShellScope::resolve_cwd(&cwd);
        if detected != self.scope {
            self.reload_scope(detected);
        }
    }

    fn prepare_compile_run(&mut self, params: &str) {
        self.try_refresh_scope();
        let scope = ShellScope::resolve_for_cli(&self.scope, params);
        if scope != self.scope {
            self.reload_scope(scope);
        }
        self.shell_state.reset_compile_progress();
        let _ = switch_page(&mut self.layout, "compile_debug");
        self.sync_focus_after_page_switch();
        self.top_menu.rebuild(&self.nav, &self.layout.pages);
        init_session_logger();
    }

    fn queue_compile_or_cli(&mut self, item: &super::catalog::CommandItem, params: &str) {
        if let super::catalog::CommandItem::Cli(cli) = item
            && hi_compile::is_in_process_command(cli.id)
            && self.compile_registrar.is_some()
        {
            self.prepare_compile_run(params);
            if self.scope.is_user() {
                let _ = self.msg_tx.send(RuntimeOp::Update(ShellMessage::PushLog(
                    "Open a workspace (.bws) or project (.bproj) before building.".into(),
                )));
                self.drain_messages();
                return;
            }
            self.pending_compile = Some(HiCompileJob {
                command: cli.id.to_string(),
                params: params.to_string(),
            });
            return;
        }
        self.queue_cli(item, params);
    }

    pub(crate) fn take_pending_cli(&mut self) -> Option<CliRunPlan> {
        self.pending_cli.take()
    }

    pub(crate) fn take_pending_compile(&mut self) -> Option<HiCompileJob> {
        self.pending_compile.take()
    }

    pub(crate) fn spawn_compile_job(
        &mut self,
        job: HiCompileJob,
    ) -> Option<std::thread::JoinHandle<anyhow::Result<()>>> {
        let registrar = self.compile_registrar?;
        let msg_tx = self.msg_tx.clone();
        let scope = self.scope.clone();
        let command = job.command;
        let params = job.params;
        Some(std::thread::spawn(move || {
            registrar(HiCompileRequest {
                command: &command,
                params: &params,
                scope: &scope,
                msg_tx,
            })
        }))
    }

    pub(crate) fn on_compile_finished(&mut self, result: anyhow::Result<()>) {
        if let Err(err) = result {
            let _ = self.msg_tx.send(RuntimeOp::Update(ShellMessage::PushLog(
                err.to_string(),
            )));
        }
        if !self.shell_state.compile_complete {
            let _ = self
                .msg_tx
                .send(RuntimeOp::Update(ShellMessage::CompileComplete));
        }
        self.drain_messages();
    }

    fn handle_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::None | PaletteAction::Redraw => {}
            PaletteAction::Close => self.sync_hotkey_scope(),
            PaletteAction::Execute(item, params) => {
                self.palette.close();
                match &item {
                    super::catalog::CommandItem::Nav(nav) => {
                        self.dispatch_nav_action(nav.action.clone());
                    }
                    super::catalog::CommandItem::Contextual(_) => {
                        if item.id().starts_with("layout.") {
                            let widget = if item.id() == "layout.add"
                                || item.id() == "layout.set_widget"
                            {
                                let w = params.trim();
                                if w.is_empty() {
                                    None
                                } else {
                                    Some(w)
                                }
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
                    super::catalog::CommandItem::Cli(_) => {
                        self.queue_compile_or_cli(&item, &params);
                    }
                }
                self.sync_hotkey_scope();
            }
        }
    }

    fn handle_command_dialog_key(&mut self, key: KeyEvent) -> ShellOutcome {
        match self.command_dialog.handle_key(key) {
            CommandDialogAction::None => ShellOutcome::Continue,
            CommandDialogAction::Redraw => ShellOutcome::Redraw,
            CommandDialogAction::Close => {
                self.sync_hotkey_scope();
                ShellOutcome::Redraw
            }
            CommandDialogAction::Run => {
                if let Some(plan) = self.command_dialog.take_run_plan(&self.beskid_exe, &self.scope) {
                    self.pending_cli = Some(plan);
                }
                self.sync_hotkey_scope();
                ShellOutcome::Redraw
            }
        }
    }

    fn reload_scope(&mut self, scope: ShellScope) {
        if let Ok(layout) = layout::load_for_scope(&scope) {
            self.scope = scope;
            self.layout = layout;
            self.nav.merge_pages(&self.layout.pages);
            self.top_menu.rebuild(&self.nav, &self.layout.pages);
            self.focused_widget = self
                .layout
                .doc
                .nodes
                .values()
                .find_map(|n| n.widget.clone())
                .unwrap_or_else(|| "hi.welcome".into());
            let config = load_config(&self.scope, &self.settings);
            self.key_bindings = ShortcutBindings::load(&config, &self.settings);
        }
    }

    fn route_overlay_input(&mut self, event: ShellRealmEvent) -> ShellOutcome {
        if let ShellRealmEvent::Input(input) = event {
            if let InputEvent::Key(key) = &input
                && let Some(action) = self.handle_global_key(*key) {
                    return action;
                }
            let result = views::on_input(&input, &mut self.shell_state);
            match result {
                InputResult::Quit => {
                    self.quit_requested = true;
                    ShellOutcome::Quit
                }
                InputResult::CloseOverlay => {
                    self.shell_state.close_focused_overlay();
                    ShellOutcome::Redraw
                }
                _ => ShellOutcome::Redraw,
            }
        } else {
            ShellOutcome::Redraw
        }
    }

    fn handle_global_key(&mut self, key: KeyEvent) -> Option<ShellOutcome> {
        if self.key_bindings.opens_palette(&key) {
            self.open_palette();
            return Some(ShellOutcome::Redraw);
        }
        if self.key_bindings.toggles_help(&key) {
            self.chrome.show_help = !self.chrome.show_help;
            return Some(ShellOutcome::Redraw);
        }
        if self.key_bindings.quits(&key) {
            self.quit_requested = true;
            return Some(ShellOutcome::Quit);
        }
        if self.layout.editor.active {
            match key.code {
                KeyCode::Char('w') => {
                    self.toggle_layout_drawer();
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Esc => {
                    let _ = self.layout.apply_command(
                        LayoutEditCommand::ToggleEdit,
                        &self.scope,
                        None,
                    );
                    self.sync_hotkey_scope();
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    let _ = self.layout.apply_command(
                        LayoutEditCommand::ResizePlus,
                        &self.scope,
                        None,
                    );
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    let _ = self.layout.apply_command(
                        LayoutEditCommand::ResizeMinus,
                        &self.scope,
                        None,
                    );
                    return Some(ShellOutcome::Redraw);
                }
                _ => {}
            }
        }
        None
    }

    fn route_widget_input(&mut self, key: KeyEvent) -> Option<ShellOutcome> {
        let widget_id = self.focused_widget.clone();
        let action = {
            let scope = &self.scope;
            let layout_doc = &self.layout.doc;
            let shell_state = &mut self.shell_state;
            let palette = &mut self.palette;
            let focused = &self.focused_widget;
            let beskid_exe = &self.beskid_exe;
            let key_bindings = &mut self.key_bindings;
            if let Some(widget) = self.registry.get_mut(&widget_id) {
                let mut ctx = WidgetContext::new(
                    scope,
                    layout_doc,
                    shell_state,
                    palette,
                    focused,
                    beskid_exe,
                    key_bindings,
                );
                widget.on_input(&super::input::ShellInput::Key(key), &mut ctx)
            } else {
                ShellAction::None
            }
        };
        match action {
            ShellAction::Quit => {
                self.quit_requested = true;
                Some(ShellOutcome::Quit)
            }
            ShellAction::OpenPalette => {
                self.open_palette();
                Some(ShellOutcome::Redraw)
            }
            ShellAction::OpenOverlay(id) => {
                self.open_overlay(id);
                Some(ShellOutcome::Redraw)
            }
            ShellAction::RunContextual(id) => {
                self.run_contextual(id);
                Some(ShellOutcome::Redraw)
            }
            ShellAction::Redraw => Some(ShellOutcome::Redraw),
            ShellAction::None => None,
        }
    }
}

impl HiShellApp {
    pub(crate) fn handle_shell_event(&mut self, event: ShellRealmEvent) -> ShellOutcome {
        self.drain_messages();

        if let ShellRealmEvent::Input(InputEvent::Mouse(mouse)) = &event {
            let action = self.handle_mouse(mouse, self.last_frame_area());
            if action != ShellOutcome::Continue {
                return action;
            }
        }

        if self.top_menu.is_active() || self.top_menu.dropdown_open() {
            if let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                if let Some(action) = self.handle_global_key(key) {
                    return action;
                }
                let menu_action = self.top_menu.handle_key(key, &self.key_bindings);
                self.handle_top_menu_action(menu_action);
                return ShellOutcome::Redraw;
            }
            if let ShellRealmEvent::Input(InputEvent::Mouse(_)) = event {
                return ShellOutcome::Redraw;
            }
        }

        if self.command_dialog.visible {
            if let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                return self.handle_command_dialog_key(key);
            }
            return ShellOutcome::Redraw;
        }

        if self.palette.visible {
            if let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                let action = self.palette.handle_key(key);
                self.handle_palette_action(action);
                return ShellOutcome::Redraw;
            }
            return ShellOutcome::Redraw;
        }

        if let Some(picker) = self.scope_picker.as_mut() {
            if let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                match picker.handle_key(key) {
                    ScopePickerAction::Close => self.scope_picker = None,
                    ScopePickerAction::Redraw => {}
                    ScopePickerAction::Selected(path) => {
                        let scope = resolve_picked_scope(&path);
                        self.reload_scope(scope);
                        self.scope_picker = None;
                    }
                }
                return ShellOutcome::Redraw;
            }
            return ShellOutcome::Redraw;
        }

        if let ShellRealmEvent::Input(InputEvent::Key(key)) = &event
            && self.key_bindings.toggles_menu(key)
            && !self.command_dialog.visible
            && !self.palette.visible
            && self.scope_picker.is_none()
        {
            let menu_action = self.top_menu.handle_key(*key, &self.key_bindings);
            self.handle_top_menu_action(menu_action);
            return ShellOutcome::Redraw;
        }

        if self.shell_state.focus.is_overlay() || self.shell_state.any_overlay_visible() {
            return self.route_overlay_input(event);
        }

        if self.layout.editor.active
            && let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                if let Some(action) = self.handle_global_key(key) {
                    return action;
                }
                if self.layout.editor.drawer_visible {
                    let overlay_action = self.layout_editor.handle_key(
                        key,
                        &mut self.layout.editor,
                        &self.layout.doc,
                    );
                    self.handle_layout_overlay_action(overlay_action);
                } else {
                    match key.code {
                        KeyCode::Tab | KeyCode::Down => {
                            let _ = self.layout.apply_command(
                                LayoutEditCommand::FocusNext,
                                &self.scope,
                                None,
                            );
                        }
                        KeyCode::BackTab | KeyCode::Up => {
                            let _ = self.layout.apply_command(
                                LayoutEditCommand::FocusPrev,
                                &self.scope,
                                None,
                            );
                        }
                        _ => {}
                    }
                }
                return ShellOutcome::Redraw;
            }

        match event {
            ShellRealmEvent::Input(InputEvent::Key(key)) => {
                if let Some(action) = self.handle_global_key(key) {
                    return action;
                }
                if let Some(action) = self.route_widget_input(key) {
                    return action;
                }
                let result = views::on_input(&InputEvent::Key(key), &mut self.shell_state);
                match result {
                    InputResult::Quit => {
                        self.quit_requested = true;
                        ShellOutcome::Quit
                    }
                    InputResult::CloseOverlay => {
                        self.shell_state.close_focused_overlay();
                        ShellOutcome::Redraw
                    }
                    _ => ShellOutcome::Redraw,
                }
            }
            ShellRealmEvent::Tick => {
                self.drain_messages();
                ShellOutcome::Redraw
            }
            ShellRealmEvent::Resize { width, height } => {
                self.set_frame_area(Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                });
                ShellOutcome::Redraw
            }
            ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => {
                if let Some(action) = self.route_widget_input_mouse(mouse) {
                    return action;
                }
                let result = views::on_input(&InputEvent::Mouse(mouse), &mut self.shell_state);
                match result {
                    InputResult::Quit => {
                        self.quit_requested = true;
                        ShellOutcome::Quit
                    }
                    InputResult::CloseOverlay => {
                        self.shell_state.close_focused_overlay();
                        ShellOutcome::Redraw
                    }
                    _ => ShellOutcome::Redraw,
                }
            }
        }
    }

    fn route_widget_input_mouse(&mut self, mouse: MouseEvent) -> Option<ShellOutcome> {
        if !mouse_is_click(&mouse) {
            return None;
        }
        let _ = mouse;
        None
    }

    pub(crate) fn draw_shell(&mut self, frame: &mut Frame) {
        self.drain_messages();
        self.shell_state.tick = self.shell_state.tick.wrapping_add(1);
        if self.shell_state.tick.is_multiple_of(40) {
            self.try_refresh_scope();
        }
        let area = frame.area();
        self.set_frame_area(area);
        if let Some(kind) = self.layout.runtime.focused_kind() {
            self.focused_widget = kind.to_string();
        }
        let edit_active = self.layout.editor.active;
        let highlight = self.focused_widget.clone();
        let page_title = self
            .layout
            .pages
            .page(&self.layout.active_page_id)
            .map(|p| p.title.as_str())
            .unwrap_or("Beskid Hi");

        let control_mode = self.control_mode();
        let resolved = match super::layout::resolve::resolve_panels(&mut self.layout.runtime, area)
        {
            Ok(r) => r,
            Err(_) => return,
        };

        self.pinned_header = resolved.header_area;
        self.chrome.render_pinned_top_bar(
            resolved.header_area,
            frame,
            &self.scope,
            page_title,
            &self.shell_state,
            &mut self.top_menu,
            &self.key_bindings,
        );

        for entry in resolved.frame.panels() {
            let rect = entry.rect;
            let widget_id = entry.kind.to_string();
            if widget_id == "shell.scope" {
                continue;
            }
            let mut ctx = WidgetContext::new(
                &self.scope,
                &self.layout.doc,
                &mut self.shell_state,
                &mut self.palette,
                &self.focused_widget,
                &self.beskid_exe,
                &mut self.key_bindings,
            );
            if let Some(widget) = self.registry.get(&widget_id) {
                widget.render(rect, frame, &mut ctx);
            }
            if edit_active && highlight == entry.kind as &str {
                render_edit_highlight(frame, rect);
            }
        }

        self.chrome.render_footer(
            resolved.chrome_area,
            frame,
            &self.hotkeys,
            control_mode,
            Some(self.focused_widget.as_str()),
            self.layout.editor.drawer_visible,
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
        if self.shell_state.overlay_visible(OverlayKind::CompileDebug) {
            let overlay = crate::tui::layout::overlay_rect_for(
                crate::tui::layout::OVERLAY_COMPILE_DEBUG,
                area,
            );
            self.shell_state.layout_rects.compile_debug_overlay = Some(overlay);
            crate::tui::screens::compile_debug_overlay::render(
                overlay,
                frame,
                &mut self.shell_state,
            );
        }
        if self.shell_state.overlay_visible(OverlayKind::Graph) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_GRAPH, area);
            self.shell_state.layout_rects.graph_overlay = Some(overlay);
            let mut ctx = self.widget_context();
            widgets::GraphWidget.render(overlay, frame, &mut ctx);
        }
        if self.shell_state.overlay_visible(OverlayKind::Settings) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_SETTINGS, area);
            self.shell_state.layout_rects.settings_overlay = Some(overlay);
            let scope = &self.scope;
            let layout_doc = &self.layout.doc;
            let shell_state = &mut self.shell_state;
            let palette = &mut self.palette;
            let focused_widget = &self.focused_widget;
            let beskid_exe = &self.beskid_exe;
            let key_bindings = &mut self.key_bindings;
            let mut ctx = WidgetContext::new(
                scope,
                layout_doc,
                shell_state,
                palette,
                focused_widget,
                beskid_exe,
                key_bindings,
            );
            if let Some(widget) = self.registry.get("shell.settings") {
                widget.render(overlay, frame, &mut ctx);
            }
        }
        if self.shell_state.overlay_visible(OverlayKind::Analysis) {
            let overlay =
                crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_ANALYSIS, area);
            self.shell_state.layout_rects.analysis_overlay = Some(overlay);
            let mut ctx = self.widget_context();
            widgets::draw_analysis_panel(overlay, frame, &mut ctx);
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

        if self.layout.editor.active {
            self.layout_editor.render(
                area,
                frame,
                &self.layout.editor,
                &self.layout.doc,
                self.registry.descriptors(),
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

        if self.command_dialog.visible {
            self.command_dialog.render(area, frame);
        }

        if self.top_menu.dropdown_open() {
            self.top_menu.render_dropdown(area, frame);
        }

        debug_assert_eq!(ShellLayer::DRAW_ORDER.last(), Some(&ShellLayer::TopMenuDropdown));
    }
}

fn render_edit_highlight(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let style = Style::default().bg(Color::Indexed(236));
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}
