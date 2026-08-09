use std::env;

use super::HiShellApp;
use crate::pipeline::tui::widgets::init_session_logger;
use crate::shell::key_bindings::ShortcutBindings;
use crate::shell::layout::{self, LayoutEditCommand, LayoutOverlayAction, switch_page, template_by_id};
use crate::shell::nav::NavAction;
use crate::shell::palette::{self, PaletteAction};
use crate::shell::scope::ShellScope;
use crate::shell::scope_picker::{ScopePickerMode, ScopePickerOverlay};
use crate::shell::settings::load_config;
use crate::shell::widget::ShellAction;
use crate::shell::widgets::{open_analysis, open_compile_debug, open_pckg, open_tests};
use crate::shell::workflow::{WorkflowCommand, WorkflowStage};
use crate::tui::shell::focus::OverlayKind;

impl HiShellApp {
    pub(super) fn open_palette(&mut self) {
        let items =
            self.registry.palette_commands(&self.scope, self.layout.editor.active, &self.nav, &self.layout.pages);
        self.palette.open(items);
        self.sync_hotkey_scope();
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

    pub(super) fn open_overlay(&mut self, widget_id: &str) {
        let mut ctx = self.widget_context();
        match widget_id {
            "pckg.browser" => open_pckg(&mut ctx),
            "tests.runner" => open_tests(&mut ctx),
            "templates.picker" => {
                ctx.shell_state.set_overlay_visible(OverlayKind::Templates, true);
                ctx.shell_state.focus_overlay(OverlayKind::Templates);
            }
            "graph.deps" => {
                ctx.shell_state.set_overlay_visible(OverlayKind::Graph, true);
                ctx.shell_state.focus_overlay(OverlayKind::Graph);
            }
            "compile.debugger" => open_compile_debug(&mut ctx),
            "analysis.diagnostics" => open_analysis(&mut ctx),
            "shell.settings" => {
                ctx.shell_state.set_overlay_visible(OverlayKind::Settings, true);
                ctx.shell_state.focus_overlay(OverlayKind::Settings);
            }
            _ => {}
        }
    }

    pub(super) fn handle_layout_overlay_action(&mut self, action: LayoutOverlayAction) {
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
                let _ = self.layout.apply_command(LayoutEditCommand::SetWidget, &self.scope, Some(widget_id));
            }
            LayoutOverlayAction::AddWidget(widget_id) => {
                let _ = self.layout.apply_command(LayoutEditCommand::AddPanel, &self.scope, Some(widget_id));
            }
            LayoutOverlayAction::LoadBoard(path) => {
                if let Ok((doc, runtime)) =
                    layout::load::load_from_source(&std::fs::read_to_string(&path).unwrap_or_default())
                {
                    self.layout.doc = doc;
                    self.layout.runtime = runtime;
                    self.layout.mark_dirty();
                }
            }
            LayoutOverlayAction::FocusNode(node_id) => {
                if let Some(node) = self.layout.doc.nodes.get(&node_id)
                    && let Some(widget) = &node.widget
                {
                    let _ = super::layout::resolve::focus_panel_by_kind(&mut self.layout.runtime, widget);
                }
            }
        }
    }

    pub(super) fn run_contextual(&mut self, id: &str) {
        match id {
            "ctx.palette" => self.open_palette(),
            "ctx.layout_edit" => {
                let was_active = self.layout.editor.active;
                let _ = self.layout.apply_command(LayoutEditCommand::ToggleEdit, &self.scope, None);
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
                let _ = self.layout.apply_command(LayoutEditCommand::FocusNext, &self.scope, None);
            }
            "layout.focus_prev" => {
                let _ = self.layout.apply_command(LayoutEditCommand::FocusPrev, &self.scope, None);
            }
            "layout.add" => {
                let _ = self.layout.apply_command(LayoutEditCommand::AddPanel, &self.scope, None);
            }
            "layout.remove" => {
                let _ = self.layout.apply_command(LayoutEditCommand::RemovePanel, &self.scope, None);
            }
            "layout.wrap_col" => {
                let _ = self.layout.apply_command(LayoutEditCommand::WrapCol, &self.scope, None);
            }
            "layout.wrap_row" => {
                let _ = self.layout.apply_command(LayoutEditCommand::WrapRow, &self.scope, None);
            }
            "layout.tabs" => {
                let _ = self.layout.apply_command(LayoutEditCommand::ConvertTabs, &self.scope, None);
            }
            "layout.stack" => {
                let _ = self.layout.apply_command(LayoutEditCommand::ConvertStack, &self.scope, None);
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

    pub(super) fn try_refresh_scope(&mut self) {
        let Ok(cwd) = env::current_dir() else {
            return;
        };
        let detected = ShellScope::resolve_cwd(&cwd);
        if detected != self.scope {
            self.reload_scope(detected);
        }
    }

    fn submit_workflow(&mut self, command: WorkflowCommand) {
        if matches!(command, WorkflowCommand::Build { .. } | WorkflowCommand::Test { .. }) {
            self.try_refresh_scope();
            self.shell_state.reset_compile_progress();
            let _ = switch_page(&mut self.layout, "compile_debug");
            self.sync_focus_after_page_switch();
            init_session_logger();
        }
        self.workflow_engine.submit(command, self.scope.clone());
    }

    pub(super) fn handle_palette_action(&mut self, action: PaletteAction) {
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
                    super::catalog::CommandItem::Workflow(wf) => {
                        let command = match wf.stage {
                            WorkflowStage::Build => WorkflowCommand::Build { params: params.clone() },
                            WorkflowStage::Test => WorkflowCommand::Test { params: params.clone() },
                            WorkflowStage::Run => WorkflowCommand::Run { target: params.clone(), args: vec![] },
                            WorkflowStage::Analyze => WorkflowCommand::Analyze { params: params.clone() },
                            WorkflowStage::Graph => WorkflowCommand::Graph { params: params.clone() },
                        };
                        self.submit_workflow(command);
                    }
                }
                self.sync_hotkey_scope();
            }
        }
    }

    pub(super) fn reload_scope(&mut self, scope: ShellScope) {
        if let Ok(layout) = layout::load_for_scope(&scope) {
            self.scope = scope;
            self.layout = layout;
            self.nav.merge_pages(&self.layout.pages);
            self.focused_widget =
                self.layout.doc.nodes.values().find_map(|n| n.widget.clone()).unwrap_or_else(|| "hi.welcome".into());
            let config = load_config(&self.scope, &self.settings);
            self.key_bindings = ShortcutBindings::load(&config, &self.settings);
            self.hotkeys.rebuild_from_bindings(&self.key_bindings);
        }
    }
}
