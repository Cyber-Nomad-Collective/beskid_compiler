//! Pluggable Beskid shell API (stable extension surface for `beskid hi` and tooling crates).

pub mod board;
pub mod catalog;
pub mod descriptor;
pub mod layout;
pub mod chrome;
pub mod context;
pub mod host;
pub mod hotkeys;
pub mod input;
pub mod palette;
pub mod registry;
pub mod scope;
pub mod scope_picker;
pub mod widget;
pub mod widgets;

pub use board::{BoardLayout, BoardRegion, BoardTile};
pub use descriptor::WidgetDescriptor;
pub use layout::{BoardV2Doc, HiLayoutState, LayoutEditCommand};
pub use catalog::{CliCommandDef, CommandItem, CommandKind, ContextualCommand};
pub use chrome::ShellChrome;
pub use context::WidgetContext;
pub use host::{ShellHost, WidgetRegistrar};
pub use hotkeys::ShellHotkeys;
pub use input::ShellInput;
pub use palette::CommandPaletteState;
pub use registry::WidgetRegistry;
pub use scope::{ShellScope, user_board_path, user_data_dir};
pub use widget::{BeskidWidget, ShellAction, WidgetMeta};
