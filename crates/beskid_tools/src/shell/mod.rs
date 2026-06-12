//! Pluggable Beskid shell API (stable extension surface for `beskid hi` and tooling crates).

pub mod board;
pub mod catalog;
pub mod cli_run;
pub mod command_dialog;
pub mod control_mode;
pub mod descriptor;
pub mod layout;
pub mod chrome;
pub mod context;
pub mod hi_compile;
pub mod host;
pub mod hotkeys;
pub mod key_bindings;
pub mod layers;
pub mod input;
pub mod nav;
pub mod phase;
pub mod top_menu;
pub mod panel_style;
pub mod platform_shortcuts;
pub mod palette;
pub mod primitives;
pub mod registry;
pub mod scope;
pub mod scope_picker;
pub mod settings;
pub mod widget;
pub mod widgets;

pub use board::{BoardLayout, BoardRegion, BoardTile};
pub use descriptor::WidgetDescriptor;
pub use layout::{BoardV2Doc, HiLayoutState, LayoutEditCommand, PagesDoc, switch_page};
pub use nav::{NavAction, NavItemDescriptor, NavRegistrar, NavRegistry, BUILTIN_NAV};
pub use phase::transition_label;
pub use top_menu::{ShellTopMenu, TopMenuAction};
pub use catalog::{CliCommandDef, CommandItem, CommandKind, ContextualCommand, NavCommandDef};
pub use cli_run::{plan_cli_command, plan_external_command, run_cli_plan, CliRunPlan};
pub use command_dialog::{CommandDialogAction, CommandDialogOverlay};
pub use control_mode::HiControlMode;
pub use chrome::ShellChrome;
pub use context::WidgetContext;
pub use hi_compile::{HiCompileJob, HiCompileRegistrar, HiCompileRequest, is_in_process_command};
pub use host::ShellHost;
pub use host::WidgetRegistrar;
pub use hotkeys::ShellHotkeys;
pub use key_bindings::{
    BindableAction, KeyChord, ShortcutBindings, BINDABLE_ACTIONS, chord_from_key, display_chord,
    encode_chord, parse_chord,
};
pub use layers::ShellLayer;
pub use input::ShellInput;
pub use palette::CommandPaletteState;
pub use registry::WidgetRegistry;
pub use scope::{ShellScope, user_board_path, user_data_dir, user_pages_path};
pub use settings::{
    SettingKind, ToolSettingDescriptor, ToolSettingsPage, ToolSettingsRegistrar,
    ToolSettingsRegistry, ToolsConfig, BUILTIN_SETTINGS, emit_config, get_value, load_config,
    parse_config, save_config, save_path_for_scope, scope_config_path, set_value, user_config_path,
};
pub use widget::{BeskidWidget, ShellAction, WidgetMeta};
