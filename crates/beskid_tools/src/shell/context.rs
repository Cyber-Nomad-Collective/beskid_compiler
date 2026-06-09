//! Shared context passed to widgets during input and render.

use std::path::PathBuf;

use super::layout::BoardV2Doc;
use super::palette::CommandPaletteState;
use super::scope::ShellScope;
use crate::tui::shell::state::ShellState;

pub struct WidgetContext<'a> {
    pub scope: &'a ShellScope,
    pub layout: &'a BoardV2Doc,
    pub shell_state: &'a mut ShellState,
    pub palette: &'a mut CommandPaletteState,
    pub focused_widget: &'a str,
    pub beskid_exe: &'a PathBuf,
}

impl<'a> WidgetContext<'a> {
    pub fn new(
        scope: &'a ShellScope,
        layout: &'a BoardV2Doc,
        shell_state: &'a mut ShellState,
        palette: &'a mut CommandPaletteState,
        focused_widget: &'a str,
        beskid_exe: &'a PathBuf,
    ) -> Self {
        Self {
            scope,
            layout,
            shell_state,
            palette,
            focused_widget,
            beskid_exe,
        }
    }
}
