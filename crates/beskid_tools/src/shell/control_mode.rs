//! Hi shell control modes — each mode exposes a distinct keyboard / footer surface.

use super::primitives::HotkeyItem;

/// Active interaction mode for `beskid hi`. Derived from overlay state in the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HiControlMode {
    #[default]
    Normal,
    /// Top menu bar (`F10`; macOS: `⌘M` / `Fn+F10`).
    TopMenu,
    /// Command palette (`Ctrl+P` / `⌘P` / `:`).
    Palette,
    /// Layout editor (`layout edit` contextual command).
    LayoutEdit,
    /// CLI argument dialog after picking a subprocess command.
    CommandDialog,
}

impl HiControlMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::TopMenu => "menu",
            Self::Palette => "palette",
            Self::LayoutEdit => "layout edit",
            Self::CommandDialog => "command",
        }
    }

    /// Mode-specific footer hints appended after global / widget hotkeys.
    pub fn footer_items(self, layout_drawer_visible: bool) -> Vec<HotkeyItem> {
        match self {
            Self::Normal => Vec::new(),
            Self::TopMenu => vec![
                HotkeyItem::new("←/→", "menu"),
                HotkeyItem::new("↓/Enter", "open"),
                HotkeyItem::new("Esc", "close"),
            ],
            Self::Palette => vec![
                HotkeyItem::new("↑/↓", "filter list"),
                HotkeyItem::new("Enter", "select"),
                HotkeyItem::new("Esc", "close"),
            ],
            Self::LayoutEdit => {
                let mut items = vec![
                    HotkeyItem::new(
                        "w",
                        if layout_drawer_visible {
                            "Hide widget list"
                        } else {
                            "Widget list"
                        },
                    ),
                    HotkeyItem::new("Tab", "Next panel"),
                    HotkeyItem::new("+/-", "resize"),
                    HotkeyItem::new("Esc", "exit edit"),
                ];
                if layout_drawer_visible {
                    items.push(HotkeyItem::new("↑/↓", "drawer list"));
                }
                items
            }
            Self::CommandDialog => vec![
                HotkeyItem::new("Enter", "run"),
                HotkeyItem::new("Esc", "cancel"),
            ],
        }
    }
}
