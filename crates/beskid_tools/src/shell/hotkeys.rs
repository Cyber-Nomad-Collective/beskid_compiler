//! Scoped hotkey registry for shell chrome and widgets.

use super::key_bindings::ShortcutBindings;
use super::primitives::{Hotkey, HotkeyItem, HotkeyRegistry, HotkeyScope};

use super::control_mode::HiControlMode;

pub struct ShellHotkeys {
    registry: HotkeyRegistry,
    active_scope: HotkeyScope,
}

impl Default for ShellHotkeys {
    fn default() -> Self {
        Self::from_bindings(&ShortcutBindings::platform_defaults())
    }
}

impl ShellHotkeys {
    pub fn from_bindings(bindings: &ShortcutBindings) -> Self {
        let mut registry = HotkeyRegistry::new();
        registry.register(
            Hotkey::new(leak_static(&bindings.palette_hint()), "Command palette")
                .scope(HotkeyScope::Global),
        );
        registry.register(
            Hotkey::new(leak_static(&bindings.label_for("help")), "Shortcut help")
                .scope(HotkeyScope::Global),
        );
        registry.register(
            Hotkey::new(leak_static(&bindings.label_for("quit")), "Quit")
                .scope(HotkeyScope::Global),
        );
        Self {
            registry,
            active_scope: HotkeyScope::Global,
        }
    }

    pub fn rebuild_from_bindings(&mut self, bindings: &ShortcutBindings) {
        let widget_hotkeys: Vec<Hotkey> = self
            .registry
            .get_hotkeys()
            .iter()
            .filter(|hk| !matches!(hk.scope, HotkeyScope::Global))
            .cloned()
            .collect();
        *self = Self::from_bindings(bindings);
        for hk in widget_hotkeys {
            self.registry.register(hk);
        }
        self.active_scope = HotkeyScope::Global;
    }

    pub fn registry(&self) -> &HotkeyRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut HotkeyRegistry {
        &mut self.registry
    }

    pub fn set_widget_scope(&mut self, widget_id: &str) {
        self.active_scope = HotkeyScope::Tab(leak_static(widget_id));
    }

    pub fn set_global(&mut self) {
        self.active_scope = HotkeyScope::Global;
    }

    pub fn set_control_mode(&mut self, mode: HiControlMode) {
        self.active_scope = match mode {
            HiControlMode::Normal => HotkeyScope::Global,
            HiControlMode::Palette => HotkeyScope::Modal("palette"),
            HiControlMode::LayoutEdit => HotkeyScope::Modal("layout-edit"),
        };
    }

    pub fn register_widget_hotkeys(&mut self, widget_id: &str, hotkeys: Vec<Hotkey>) {
        for mut hk in hotkeys {
            hk.scope = HotkeyScope::Tab(leak_static(widget_id));
            self.registry.register(hk);
        }
    }

    pub fn footer_items(&self, widget_id: Option<&str>) -> Vec<HotkeyItem> {
        let mut items = Vec::new();
        for hk in self.registry.get_hotkeys() {
            if hk.scope == HotkeyScope::Global
                || widget_id.is_some_and(|id| hk.scope == HotkeyScope::Tab(leak_static(id)))
            {
                items.push(HotkeyItem::new(hk.key.clone(), hk.description.clone()));
            }
        }
        items
    }

    /// Footer for the active control mode: modal modes replace widget hotkeys.
    pub fn footer_for_mode(
        &self,
        mode: HiControlMode,
        widget_id: Option<&str>,
        layout_drawer_visible: bool,
    ) -> Vec<HotkeyItem> {
        match mode {
            HiControlMode::Normal => self.footer_items(widget_id),
            other => other.footer_items(layout_drawer_visible),
        }
    }
}

fn leak_static(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn footer_reflects_rebound_palette_label() {
        let mut bindings = ShortcutBindings::platform_defaults();
        bindings.palette = super::super::key_bindings::KeyChord {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::CONTROL,
        };
        let hotkeys = ShellHotkeys::from_bindings(&bindings);
        let palette = hotkeys
            .footer_items(None)
            .into_iter()
            .find(|item| item.description == "Command palette")
            .expect("palette footer item");
        assert!(palette.key.contains('k'));
    }
}
