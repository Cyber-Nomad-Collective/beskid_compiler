//! Extension widgets for `beskid hi`.

mod models;
mod register;
mod widgets;

pub use models::board_fragment::BOARD_FRAGMENT_V2;
pub use models::descriptor::{ExtensionWidgetDescriptor, WIDGET_CATALOG};
pub use models::nav::{ExtensionNavItem, NAV_CATALOG};
pub use register::{register_nav, register_widgets};
pub use widgets::HelloWidget;
