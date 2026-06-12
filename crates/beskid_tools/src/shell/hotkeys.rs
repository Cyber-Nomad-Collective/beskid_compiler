//! Scoped hotkey registry for shell chrome and widgets.

use super::primitives::{Hotkey, HotkeyItem, HotkeyRegistry, HotkeyScope};

use super::control_mode::HiControlMode;
use super::platform_shortcuts;

pub struct ShellHotkeys {
    registry: HotkeyRegistry,
    active_scope: HotkeyScope,
}

impl Default for ShellHotkeys {
    fn default() -> Self {
        let mut registry = HotkeyRegistry::new();
        registry.register(
            Hotkey::new(platform_shortcuts::palette_label(), "Command palette")
                .scope(HotkeyScope::Global),
        );
        registry.register(
            Hotkey::new(platform_shortcuts::menu_label(), "Top menu")
                .scope(HotkeyScope::Global),
        );
        if platform_shortcuts::is_macos() {
            registry.register(
                Hotkey::new("⌘M", "Top menu")
                    .scope(HotkeyScope::Global),
            );
        }
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

    pub fn set_control_mode(&mut self, mode: HiControlMode) {
        self.active_scope = match mode {
            HiControlMode::Normal => HotkeyScope::Global,
            HiControlMode::TopMenu => HotkeyScope::Modal("menu"),
            HiControlMode::Palette => HotkeyScope::Modal("palette"),
            HiControlMode::LayoutEdit => HotkeyScope::Modal("layout-edit"),
            HiControlMode::CommandDialog => HotkeyScope::Modal("command"),
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
