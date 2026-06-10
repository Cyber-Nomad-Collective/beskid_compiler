//! Shell UI primitives (ratkit widgets; tuirealm owns the event loop).

pub use ratkit::services::hotkey_service::{Hotkey, HotkeyRegistry, HotkeyScope};
pub use ratkit::widgets::{HotkeyFooter, HotkeyItem, TreeNavigator, TreeNode, TreeViewState};
