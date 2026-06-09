//! Scoped hotkey registry for shell chrome and widgets.

use ratkit::services::hotkey_service::{Hotkey, HotkeyRegistry, HotkeyScope};
use ratkit::widgets::HotkeyItem;

pub struct ShellHotkeys {
    registry: HotkeyRegistry,
    active_scope: HotkeyScope,
}

impl Default for ShellHotkeys {
    fn default() -> Self {
        let mut registry = HotkeyRegistry::new();
        registry.register(
            Hotkey::new("Ctrl+P", "Command palette")
                .scope(HotkeyScope::Global),
        );
        registry.register(Hotkey::new(":", "Command palette").scope(HotkeyScope::Global));
        registry.register(Hotkey::new("?", "Shortcut help").scope(HotkeyScope::Global));
        registry.register(Hotkey::new("q", "Quit").scope(HotkeyScope::Global));
        Self {
            registry,
            active_scope: HotkeyScope::Global,
        }
    }
}

impl ShellHotkeys {
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
}

fn leak_static(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}
