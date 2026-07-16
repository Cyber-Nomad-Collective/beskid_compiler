//! Shell host: `beskid hi` entry and tuirealm runtime.

use std::env;
use std::io::{self, IsTerminal, stderr};
use std::sync::mpsc::{self, Receiver, Sender};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::chrome::ShellChrome;
use super::context::WidgetContext;
use super::control_mode::HiControlMode;
use super::hotkeys::ShellHotkeys;
use super::key_bindings::ShortcutBindings;
use super::layers::ShellLayer;
use super::layout::{
    self, HiLayoutState, LayoutEditCommand, LayoutEditorOverlay, LayoutOverlayAction, switch_page,
    template_by_id,
};
use super::nav::NavAction;
use super::nav::{NavRegistrar, NavRegistry};
use super::overlay_render::{self, HiOverlayWidgets, OverlayRenderContext};
use super::palette::{self, CommandPaletteState, PaletteAction};
use super::registry::WidgetRegistry;
use super::scope::ShellScope;
use super::scope_picker::{
    ScopePickerAction, ScopePickerMode, ScopePickerOverlay, resolve_picked_scope,
};
use super::settings::{ToolSettingsRegistrar, ToolSettingsRegistry, load_config};
use super::shortcut_clicks::{ShortcutClickAction, ShortcutClickTargets};
use super::widget::ShellAction;
use super::widgets::{self, open_analysis, open_compile_debug, open_pckg, open_tests};
use super::workflow::{WorkflowCommand, WorkflowEngine, WorkflowStage};
use crate::pipeline::tui::widgets::init_session_logger;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::realm::shell_event::{
    ShellOutcome, ShellRealmEvent, mouse_is_click, mouse_is_inside, mouse_is_move_or_drag,
};
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
        widget_registrars: &[WidgetRegistrar],
        nav_registrars: &[NavRegistrar],
        settings_registrars: &[ToolSettingsRegistrar],
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
        let app = HiShellApp::new(scope, layout, registry, nav, settings);
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
    pub quit_requested: bool,
    pub scope_picker: Option<ScopePickerOverlay>,
    pub layout_editor: LayoutEditorOverlay,
    pub workflow_engine: WorkflowEngine,
    key_bindings: ShortcutBindings,
    shortcut_clicks: ShortcutClickTargets,
    pending_shortcut_rebind: Option<usize>,
    pinned_header: Rect,
    frame_area: Rect,
    pub(crate) msg_tx: Sender<RuntimeOp>,
    msg_rx: Receiver<RuntimeOp>,
}

impl HiShellApp {
    pub fn new(
        scope: ShellScope,
        mut layout: HiLayoutState,
        registry: WidgetRegistry,
        nav: NavRegistry,
        settings: ToolSettingsRegistry,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        let mut scope = scope;
        if scope.is_user()
            && let Ok(cwd) = env::current_dir()
        {
            scope = ShellScope::resolve_cwd(&cwd);
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
        let config = load_config(&scope, &settings);
        let key_bindings = ShortcutBindings::load(&config, &settings);
        let hotkeys = ShellHotkeys::from_bindings(&key_bindings);
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
            quit_requested: false,
            scope_picker: None,
            layout_editor: LayoutEditorOverlay::default(),
            workflow_engine: WorkflowEngine::default(),
            key_bindings,
            shortcut_clicks: ShortcutClickTargets::default(),
            pending_shortcut_rebind: None,
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
        if self.palette.visible {
            HiControlMode::Palette
        } else if self.layout.editor.active {
            HiControlMode::LayoutEdit
        } else {
            HiControlMode::Normal
        }
    }

    fn sync_hotkey_scope(&mut self) {
        self.hotkeys.rebuild_from_bindings(&self.key_bindings);
        self.hotkeys.set_control_mode(self.control_mode());
    }

    fn layer_is_active(&self, layer: ShellLayer) -> bool {
        match layer {
            ShellLayer::Palette => self.palette.visible,
            ShellLayer::ScopePicker => self.scope_picker.is_some(),
            ShellLayer::LayoutEditor => self.layout.editor.active,
            ShellLayer::PanelOverlay => {
                self.shell_state.focus.is_overlay() || self.shell_state.any_overlay_visible()
            }
            ShellLayer::Help => self.chrome.show_help,
            ShellLayer::Base => false,
        }
    }

    fn layer_blocks_mouse(&self, layer: ShellLayer) -> bool {
        self.layer_is_active(layer)
    }

    fn top_input_layer(&self) -> Option<ShellLayer> {
        ShellLayer::INPUT_PRIORITY
            .iter()
            .copied()
            .find(|layer| self.layer_is_active(*layer))
    }

    fn top_mouse_layer(&self) -> Option<ShellLayer> {
        ShellLayer::INPUT_PRIORITY
            .iter()
            .copied()
            .find(|layer| self.layer_blocks_mouse(*layer))
    }

    fn handle_modal_mouse(&mut self, _mouse: &MouseEvent) -> Option<ShellOutcome> {
        match self.top_mouse_layer()? {
            ShellLayer::Palette | ShellLayer::ScopePicker | ShellLayer::PanelOverlay => {
                Some(ShellOutcome::Redraw)
            }
            ShellLayer::LayoutEditor | ShellLayer::Help | ShellLayer::Base => None,
        }
    }

    fn modal_mouse_outcome(&self, mouse: &MouseEvent) -> Option<ShellOutcome> {
        if mouse_is_move_or_drag(mouse) {
            None
        } else {
            Some(ShellOutcome::Redraw)
        }
    }

    fn handle_modal_input(&mut self, event: &ShellRealmEvent) -> Option<ShellOutcome> {
        let layer = self.top_input_layer()?;
        match layer {
            ShellLayer::Palette => match event {
                ShellRealmEvent::Input(InputEvent::Key(key)) => {
                    let action = self.palette.handle_key(*key);
                    self.handle_palette_action(action);
                    Some(ShellOutcome::Redraw)
                }
                ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => self.modal_mouse_outcome(mouse),
                _ => Some(ShellOutcome::Redraw),
            },
            ShellLayer::ScopePicker => {
                let picker = self.scope_picker.as_mut()?;
                match event {
                    ShellRealmEvent::Input(InputEvent::Key(key)) => {
                        match picker.handle_key(*key) {
                            ScopePickerAction::Close => self.scope_picker = None,
                            ScopePickerAction::Redraw => {}
                            ScopePickerAction::Selected(path) => {
                                let scope = resolve_picked_scope(&path);
                                self.reload_scope(scope);
                                self.scope_picker = None;
                            }
                        }
                        Some(ShellOutcome::Redraw)
                    }
                    ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => {
                        self.modal_mouse_outcome(mouse)
                    }
                    _ => Some(ShellOutcome::Redraw),
                }
            }
            ShellLayer::LayoutEditor => {
                let ShellRealmEvent::Input(InputEvent::Key(key)) = event else {
                    return None;
                };
                if let Some(action) = self.handle_global_key(*key) {
                    return Some(action);
                }
                if self.layout.editor.drawer_visible {
                    let overlay_action = self.layout_editor.handle_key(
                        *key,
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
                Some(ShellOutcome::Redraw)
            }
            ShellLayer::PanelOverlay => Some(self.route_overlay_input(event.clone())),
            ShellLayer::Help | ShellLayer::Base => None,
        }
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

        if let Some(outcome) = self.handle_modal_mouse(mouse) {
            return outcome;
        }

        if let Some(action) = self.shortcut_clicks.hit(mouse.column, mouse.row) {
            return self.dispatch_shortcut_click(action);
        }

        if self.layout.editor.active {
            if self.layout.editor.drawer_visible {
                let drawer = self.layout_drawer_rect(area);
                if mouse_is_inside(mouse, drawer) {
                    return ShellOutcome::Redraw;
                }
            }
            let panel_id = super::layout::resolve::resolve_panels(&mut self.layout.runtime, area)
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
            &mut self.key_bindings,
            &mut self.shortcut_clicks,
            &mut self.pending_shortcut_rebind,
        )
    }

    fn dispatch_shortcut_click(&mut self, action: ShortcutClickAction) -> ShellOutcome {
        match action {
            ShortcutClickAction::OpenPalette => {
                self.open_palette();
                ShellOutcome::Redraw
            }
            ShortcutClickAction::ToggleHelp => {
                self.chrome.show_help = !self.chrome.show_help;
                ShellOutcome::Redraw
            }
            ShortcutClickAction::Quit => {
                self.quit_requested = true;
                ShellOutcome::Quit
            }
            ShortcutClickAction::RebindShortcut(index) => {
                self.pending_shortcut_rebind = Some(index);
                ShellOutcome::Redraw
            }
        }
    }

    fn drain_messages(&mut self) -> bool {
        let mut changed = false;
        while let Ok(op) = self.msg_rx.try_recv() {
            if let RuntimeOp::Update(msg) = op {
                let effects = views::update(&msg, &mut self.shell_state);
                apply_effects(effects, &self.msg_tx, &mut self.shell_state);
                changed = true;
            }
        }
        drain_pending_work(&self.msg_tx, &mut self.shell_state);
        let _ = self.layout.maybe_autosave(&self.scope);
        changed
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
        for widget in [
            "hi.welcome",
            "graph.deps",
            "compile.debugger",
            "analysis.diagnostics",
        ] {
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
            NavAction::Group | NavAction::Cli(_) => {}
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
                ctx.shell_state
                    .set_overlay_visible(OverlayKind::Graph, true);
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
                    && let Some(widget) = &node.widget
                {
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
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::FocusNext, &self.scope, None);
            }
            "layout.focus_prev" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::FocusPrev, &self.scope, None);
            }
            "layout.add" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::AddPanel, &self.scope, None);
            }
            "layout.remove" => {
                let _ =
                    self.layout
                        .apply_command(LayoutEditCommand::RemovePanel, &self.scope, None);
            }
            "layout.wrap_col" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::WrapCol, &self.scope, None);
            }
            "layout.wrap_row" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::WrapRow, &self.scope, None);
            }
            "layout.tabs" => {
                let _ =
                    self.layout
                        .apply_command(LayoutEditCommand::ConvertTabs, &self.scope, None);
            }
            "layout.stack" => {
                let _ =
                    self.layout
                        .apply_command(LayoutEditCommand::ConvertStack, &self.scope, None);
            }
            "layout.save" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::Save, &self.scope, None);
            }
            "layout.reset" => {
                let _ = self
                    .layout
                    .apply_command(LayoutEditCommand::Reset, &self.scope, None);
                self.nav.merge_pages(&self.layout.pages);
            }
            _ => {}
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

    fn submit_workflow(&mut self, command: WorkflowCommand) {
        if matches!(
            command,
            WorkflowCommand::Build { .. } | WorkflowCommand::Test { .. }
        ) {
            self.try_refresh_scope();
            self.shell_state.reset_compile_progress();
            let _ = switch_page(&mut self.layout, "compile_debug");
            self.sync_focus_after_page_switch();
            init_session_logger();
        }
        self.workflow_engine.submit(command, self.scope.clone());
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
                            let widget =
                                if item.id() == "layout.add" || item.id() == "layout.set_widget" {
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
                    super::catalog::CommandItem::Workflow(wf) => {
                        let command = match wf.stage {
                            WorkflowStage::Build => WorkflowCommand::Build {
                                params: params.clone(),
                            },
                            WorkflowStage::Test => WorkflowCommand::Test {
                                params: params.clone(),
                            },
                            WorkflowStage::Run => WorkflowCommand::Run {
                                target: params.clone(),
                                args: vec![],
                            },
                            WorkflowStage::Analyze => WorkflowCommand::Analyze {
                                params: params.clone(),
                            },
                            WorkflowStage::Graph => WorkflowCommand::Graph {
                                params: params.clone(),
                            },
                        };
                        self.submit_workflow(command);
                    }
                }
                self.sync_hotkey_scope();
            }
        }
    }

    fn reload_scope(&mut self, scope: ShellScope) {
        if let Ok(layout) = layout::load_for_scope(&scope) {
            self.scope = scope;
            self.layout = layout;
            self.nav.merge_pages(&self.layout.pages);
            self.focused_widget = self
                .layout
                .doc
                .nodes
                .values()
                .find_map(|n| n.widget.clone())
                .unwrap_or_else(|| "hi.welcome".into());
            let config = load_config(&self.scope, &self.settings);
            self.key_bindings = ShortcutBindings::load(&config, &self.settings);
            self.hotkeys.rebuild_from_bindings(&self.key_bindings);
        }
    }

    fn route_overlay_input(&mut self, event: ShellRealmEvent) -> ShellOutcome {
        if let ShellRealmEvent::Input(input) = event {
            if let InputEvent::Key(key) = &input
                && let Some(action) = self.handle_global_key(*key)
            {
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
                    let _ =
                        self.layout
                            .apply_command(LayoutEditCommand::ToggleEdit, &self.scope, None);
                    self.sync_hotkey_scope();
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    let _ =
                        self.layout
                            .apply_command(LayoutEditCommand::ResizePlus, &self.scope, None);
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
            let key_bindings = &mut self.key_bindings;
            let shortcut_clicks = &mut self.shortcut_clicks;
            let pending_shortcut_rebind = &mut self.pending_shortcut_rebind;
            if let Some(widget) = self.registry.get_mut(&widget_id) {
                let mut ctx = WidgetContext::new(
                    scope,
                    layout_doc,
                    shell_state,
                    palette,
                    focused,
                    key_bindings,
                    shortcut_clicks,
                    pending_shortcut_rebind,
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

        if let Some(outcome) = self.handle_modal_input(&event) {
            return outcome;
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
                let changed = self.drain_messages();
                if changed || self.shell_state.pipeline_active() {
                    ShellOutcome::Redraw
                } else {
                    ShellOutcome::Continue
                }
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
        self.shortcut_clicks.clear();
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
        let _page_title = self
            .layout
            .pages
            .page(&self.layout.active_page_id)
            .map(|p| p.title.as_str())
            .unwrap_or("Beskid Hi");

        let control_mode = self.control_mode();
        let header_h = super::chrome::PINNED_TOP_ROWS.min(area.height);
        let chrome_h = 1u16.min(area.height.saturating_sub(header_h));
        let main_h = area
            .height
            .saturating_sub(header_h)
            .saturating_sub(chrome_h);
        let header_area = ratatui::layout::Rect {
            width: area.width,
            height: header_h,
            x: area.x,
            y: area.y,
        };
        let main_area = ratatui::layout::Rect {
            width: area.width,
            height: main_h,
            x: area.x,
            y: area.y + header_h,
        };
        let chrome_area = ratatui::layout::Rect {
            width: area.width,
            height: chrome_h,
            x: area.x,
            y: area.y + header_h + main_h,
        };

        let resolved = match super::layout::resolve::resolve_panels(&mut self.layout.runtime, area)
        {
            Ok(r) => r,
            Err(message) => {
                self.pinned_header = header_area;
                self.chrome
                    .render_pinned_top_bar(header_area, frame, &self.scope);
                let error_area = if main_area.width == 0 || main_area.height == 0 {
                    area
                } else {
                    main_area
                };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(format!("Layout error: {message}"))
                        .style(ratatui::style::Style::default().fg(ratatui::style::Color::Red)),
                    error_area,
                );
                self.chrome.render_footer(
                    chrome_area,
                    frame,
                    &self.hotkeys,
                    control_mode,
                    Some(self.focused_widget.as_str()),
                    self.layout.editor.drawer_visible,
                    &mut self.shortcut_clicks,
                );
                return;
            }
        };

        self.pinned_header = resolved.header_area;
        self.chrome
            .render_pinned_top_bar(resolved.header_area, frame, &self.scope);

        for entry in resolved.frame.panels() {
            let rect = entry.rect;
            let widget_id = entry.kind.to_string();
            if widget_id == "shell.scope" {
                continue;
            }
            let key_bindings = &mut self.key_bindings;
            let shortcut_clicks = &mut self.shortcut_clicks;
            let pending_shortcut_rebind = &mut self.pending_shortcut_rebind;
            let mut ctx = WidgetContext::new(
                &self.scope,
                &self.layout.doc,
                &mut self.shell_state,
                &mut self.palette,
                &self.focused_widget,
                key_bindings,
                shortcut_clicks,
                pending_shortcut_rebind,
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
            &mut self.shortcut_clicks,
        );

        for layer in ShellLayer::DRAW_ORDER {
            match layer {
                ShellLayer::Base => {}
                ShellLayer::PanelOverlay => {
                    let scope = &self.scope;
                    let layout_doc = &self.layout.doc;
                    let shell_state = &mut self.shell_state;
                    let palette = &mut self.palette;
                    let focused_widget = &self.focused_widget;
                    let key_bindings = &mut self.key_bindings;
                    let shortcut_clicks = &mut self.shortcut_clicks;
                    let pending_shortcut_rebind = &mut self.pending_shortcut_rebind;
                    let registry = &self.registry;
                    let mut ctx = WidgetContext::new(
                        scope,
                        layout_doc,
                        shell_state,
                        palette,
                        focused_widget,
                        key_bindings,
                        shortcut_clicks,
                        pending_shortcut_rebind,
                    );
                    overlay_render::render_panel_overlays(
                        frame,
                        area,
                        OverlayRenderContext::Hi(HiOverlayWidgets {
                            ctx: &mut ctx,
                            registry,
                        }),
                    );
                }
                ShellLayer::Help => {
                    if self.chrome.show_help {
                        let help_area = crate::tui::layout::overlay_rect_for(
                            crate::tui::layout::OVERLAY_TESTS,
                            area,
                        );
                        let help_items = self.hotkeys.footer_items(Some(&self.focused_widget));
                        self.chrome.render_help_overlay(
                            help_area,
                            frame,
                            &help_items,
                            &mut self.shortcut_clicks,
                        );
                    }
                }
                ShellLayer::LayoutEditor => {
                    if self.layout.editor.active {
                        self.layout_editor.render(
                            area,
                            frame,
                            &self.layout.editor,
                            &self.layout.doc,
                            self.registry.descriptors(),
                        );
                    }
                }
                ShellLayer::ScopePicker => {
                    if let Some(picker) = &self.scope_picker {
                        let overlay = crate::tui::layout::overlay_rect_for(
                            crate::tui::layout::OVERLAY_PCKG,
                            area,
                        );
                        picker.render(overlay, frame);
                    }
                }
                ShellLayer::Palette => {
                    if self.palette.visible {
                        self.palette
                            .render(area, frame, &self.key_bindings.palette_hint());
                    }
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use super::*;
    use crate::shell::layout;
    use crate::shell::widgets;
    use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent};

    fn test_app() -> HiShellApp {
        let scope = ShellScope::User;
        let layout_state = layout::load_for_scope(&scope).expect("layout");
        let mut registry = WidgetRegistry::new();
        widgets::register_builtins(&mut registry);
        let mut nav = NavRegistry::new();
        nav.merge_pages(&layout_state.pages);
        let settings = ToolSettingsRegistry::with_builtins();
        HiShellApp::new(scope, layout_state, registry, nav, settings)
    }

    #[test]
    fn tick_idle_returns_continue() {
        let mut app = test_app();
        assert_eq!(
            app.handle_shell_event(ShellRealmEvent::Tick),
            ShellOutcome::Continue
        );
    }

    #[test]
    fn layout_resolve_error_shows_fallback() {
        let mut app = test_app();
        app.set_frame_area(Rect::new(0, 0, 40, 2));

        let backend = TestBackend::new(40, 2);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| app.draw_shell(frame)).expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("Layout error"),
            "expected fallback message in buffer: {text:?}"
        );
    }
}
