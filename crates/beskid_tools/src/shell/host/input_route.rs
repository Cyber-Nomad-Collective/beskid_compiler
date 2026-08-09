use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;

use super::HiShellApp;
use crate::shell::context::WidgetContext;
use crate::shell::layers::ShellLayer;
use crate::shell::layout::LayoutEditCommand;
use crate::shell::scope_picker::{ScopePickerAction, resolve_picked_scope};
use crate::shell::shortcut_clicks::ShortcutClickAction;
use crate::shell::widget::ShellAction;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::realm::shell_event::{
    ShellOutcome, ShellRealmEvent, mouse_is_click, mouse_is_inside, mouse_is_move_or_drag,
};
use crate::tui::views;

impl HiShellApp {
    fn handle_modal_mouse(&mut self, _mouse: &MouseEvent) -> Option<ShellOutcome> {
        match self.top_mouse_layer()? {
            ShellLayer::Palette | ShellLayer::ScopePicker | ShellLayer::PanelOverlay => Some(ShellOutcome::Redraw),
            ShellLayer::LayoutEditor | ShellLayer::Help | ShellLayer::Base => None,
        }
    }

    fn modal_mouse_outcome(&self, mouse: &MouseEvent) -> Option<ShellOutcome> {
        if mouse_is_move_or_drag(mouse) { None } else { Some(ShellOutcome::Redraw) }
    }

    pub(super) fn handle_modal_input(&mut self, event: &ShellRealmEvent) -> Option<ShellOutcome> {
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
                    ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => self.modal_mouse_outcome(mouse),
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
                    let overlay_action = self.layout_editor.handle_key(*key, &mut self.layout.editor, &self.layout.doc);
                    self.handle_layout_overlay_action(overlay_action);
                } else {
                    match key.code {
                        KeyCode::Tab | KeyCode::Down => {
                            let _ = self.layout.apply_command(LayoutEditCommand::FocusNext, &self.scope, None);
                        }
                        KeyCode::BackTab | KeyCode::Up => {
                            let _ = self.layout.apply_command(LayoutEditCommand::FocusPrev, &self.scope, None);
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

    pub(super) fn handle_mouse(&mut self, mouse: &MouseEvent, area: Rect) -> ShellOutcome {
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
            let panel_id =
                super::layout::resolve::resolve_panels(&mut self.layout.runtime, area).ok().and_then(|resolved| {
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

    pub(super) fn handle_global_key(&mut self, key: KeyEvent) -> Option<ShellOutcome> {
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
                    let _ = self.layout.apply_command(LayoutEditCommand::ToggleEdit, &self.scope, None);
                    self.sync_hotkey_scope();
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    let _ = self.layout.apply_command(LayoutEditCommand::ResizePlus, &self.scope, None);
                    return Some(ShellOutcome::Redraw);
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    let _ = self.layout.apply_command(LayoutEditCommand::ResizeMinus, &self.scope, None);
                    return Some(ShellOutcome::Redraw);
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn route_widget_input(&mut self, key: KeyEvent) -> Option<ShellOutcome> {
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

    pub(super) fn route_widget_input_mouse(&mut self, mouse: MouseEvent) -> Option<ShellOutcome> {
        if !mouse_is_click(&mouse) {
            return None;
        }
        let _ = mouse;
        None
    }
}
