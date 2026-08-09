use std::env;
use std::sync::mpsc::{self, Receiver, Sender};

use ratatui::layout::Rect;

use crate::shell::chrome::ShellChrome;
use crate::shell::context::WidgetContext;
use crate::shell::hotkeys::ShellHotkeys;
use crate::shell::key_bindings::ShortcutBindings;
use crate::shell::layout::{HiLayoutState, LayoutEditorOverlay, switch_page};
use crate::shell::nav::NavRegistry;
use crate::shell::palette::CommandPaletteState;
use crate::shell::registry::WidgetRegistry;
use crate::shell::scope::ShellScope;
use crate::shell::scope_picker::ScopePickerOverlay;
use crate::shell::settings::{ToolSettingsRegistry, load_config};
use crate::shell::shortcut_clicks::ShortcutClickTargets;
use crate::shell::workflow::WorkflowEngine;
use crate::tui::shell::effects::{apply_effects, drain_pending_work};
use crate::tui::shell::pane_state::ShellMode;
use crate::tui::shell::runtime::RuntimeOp;
use crate::tui::shell::state::ShellState;
use crate::tui::views;

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
    pub(super) key_bindings: ShortcutBindings,
    pub(super) shortcut_clicks: ShortcutClickTargets,
    pub(super) pending_shortcut_rebind: Option<usize>,
    pub(super) pinned_header: Rect,
    pub(super) frame_area: Rect,
    pub(crate) msg_tx: Sender<RuntimeOp>,
    pub(super) msg_rx: Receiver<RuntimeOp>,
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
        let shell_state = ShellState { shell_mode: ShellMode::Hi, compile_complete: true, ..Default::default() };
        let active_page = layout.active_page_id.clone();
        let _ = switch_page(&mut layout, &active_page);
        let focused = layout.doc.nodes.values().find_map(|n| n.widget.clone()).unwrap_or_else(|| "hi.welcome".into());
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

    pub(super) fn last_frame_area(&self) -> Rect {
        self.frame_area
    }

    pub(crate) fn set_frame_area(&mut self, area: Rect) {
        self.frame_area = area;
    }

    pub(super) fn widget_context(&mut self) -> WidgetContext<'_> {
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

    pub(super) fn drain_messages(&mut self) -> bool {
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
}
