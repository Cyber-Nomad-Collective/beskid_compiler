//! Pluggable Beskid shell API (stable extension surface for `beskid hi` and tooling crates).

pub mod board;
pub mod catalog;
pub mod chrome;
pub mod context;
pub mod control_mode;
pub mod descriptor;
pub mod hi_compile;
pub mod host;
pub mod hotkeys;
pub mod input;
pub mod key_bindings;
pub mod layers;
pub mod layout;
pub mod nav;
pub mod overlay_render;
pub mod palette;
pub mod panel_style;
pub mod phase;
pub mod platform_shortcuts;
pub mod primitives;
pub mod registry;
pub mod scope;
pub mod scope_picker;
pub mod settings;
pub mod shortcut_clicks;
pub mod widget;
pub mod widgets;
pub mod workflow;

pub use board::{BoardLayout, BoardRegion, BoardTile};
pub use catalog::{CommandItem, CommandKind, ContextualCommand, NavCommandDef, WorkflowCommandDef};
pub use chrome::ShellChrome;
pub use context::WidgetContext;
pub use control_mode::HiControlMode;
pub use descriptor::WidgetDescriptor;
pub use hi_compile::{HiCompileJob, HiCompileRegistrar, HiCompileRequest, is_in_process_command};
pub use host::ShellHost;
pub use host::WidgetRegistrar;
pub use hotkeys::ShellHotkeys;
pub use input::ShellInput;
pub use key_bindings::{
    BINDABLE_ACTIONS, BindableAction, KeyChord, ShortcutBindings, chord_from_key, display_chord, encode_chord,
    parse_chord,
};
pub use layers::ShellLayer;
pub use layout::{BoardV2Doc, HiLayoutState, LayoutEditCommand, PagesDoc, switch_page};
pub use nav::{BUILTIN_NAV, NavAction, NavItemDescriptor, NavRegistrar, NavRegistry};
pub use palette::CommandPaletteState;
pub use phase::transition_label;
pub use registry::WidgetRegistry;
pub use scope::{ShellScope, user_board_path, user_data_dir, user_pages_path};
pub use settings::{
    BUILTIN_SETTINGS, SettingKind, ToolSettingDescriptor, ToolSettingsPage, ToolSettingsRegistrar,
    ToolSettingsRegistry, ToolsConfig, emit_config, get_value, load_config, parse_config, save_config,
    save_path_for_scope, scope_config_path, set_value, user_config_path,
};
pub use shortcut_clicks::{ShortcutClickAction, ShortcutClickTargets};
pub use widget::{BeskidWidget, ShellAction, WidgetMeta};
