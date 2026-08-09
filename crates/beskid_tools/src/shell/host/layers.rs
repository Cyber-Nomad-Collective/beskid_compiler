use ratatui::layout::Rect;

use super::HiShellApp;
use crate::shell::control_mode::HiControlMode;
use crate::shell::layers::ShellLayer;

impl HiShellApp {
    pub(super) fn control_mode(&self) -> HiControlMode {
        if self.palette.visible {
            HiControlMode::Palette
        } else if self.layout.editor.active {
            HiControlMode::LayoutEdit
        } else {
            HiControlMode::Normal
        }
    }

    pub(super) fn sync_hotkey_scope(&mut self) {
        self.hotkeys.rebuild_from_bindings(&self.key_bindings);
        self.hotkeys.set_control_mode(self.control_mode());
    }

    pub(super) fn layer_is_active(&self, layer: ShellLayer) -> bool {
        match layer {
            ShellLayer::Palette => self.palette.visible,
            ShellLayer::ScopePicker => self.scope_picker.is_some(),
            ShellLayer::LayoutEditor => self.layout.editor.active,
            ShellLayer::PanelOverlay => self.shell_state.focus.is_overlay() || self.shell_state.any_overlay_visible(),
            ShellLayer::Help => self.chrome.show_help,
            ShellLayer::Base => false,
        }
    }

    fn layer_blocks_mouse(&self, layer: ShellLayer) -> bool {
        self.layer_is_active(layer)
    }

    pub(super) fn top_input_layer(&self) -> Option<ShellLayer> {
        ShellLayer::INPUT_PRIORITY.iter().copied().find(|layer| self.layer_is_active(*layer))
    }

    pub(super) fn top_mouse_layer(&self) -> Option<ShellLayer> {
        ShellLayer::INPUT_PRIORITY.iter().copied().find(|layer| self.layer_blocks_mouse(*layer))
    }

    pub(super) fn toggle_layout_drawer(&mut self) {
        self.layout.editor.drawer_visible = !self.layout.editor.drawer_visible;
        if self.layout.editor.drawer_visible {
            self.layout.editor.overlay_tab = super::layout::LayoutOverlayTab::Widgets;
            self.layout_editor.refresh_saved_boards(&self.scope);
        }
    }

    pub(super) fn layout_drawer_rect(&self, area: Rect) -> Rect {
        let width = (area.width as u32 * 40 / 100).max(20) as u16;
        Rect {
            x: area.x + area.width.saturating_sub(width),
            y: area.y,
            width: width.min(area.width),
            height: area.height,
        }
    }

    pub(super) fn sync_focus_after_page_switch(&mut self) {
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
}
