//! Shared context passed to widgets during input and render.

use std::path::PathBuf;

use super::board::BoardLayout;
use super::palette::CommandPaletteState;
use super::scope::ShellScope;
use crate::tui::shell::state::ShellState;

pub struct WidgetContext<'a> {
    pub scope: &'a ShellScope,
    pub board: &'a BoardLayout,
    pub shell_state: &'a mut ShellState,
    pub palette: &'a mut CommandPaletteState,
    pub focused_widget: &'a str,
    pub beskid_exe: &'a PathBuf,
}

impl<'a> WidgetContext<'a> {
    pub fn new(
        scope: &'a ShellScope,
        board: &'a BoardLayout,
        shell_state: &'a mut ShellState,
        palette: &'a mut CommandPaletteState,
        focused_widget: &'a str,
        beskid_exe: &'a PathBuf,
    ) -> Self {
        Self {
            scope,
            board,
            shell_state,
            palette,
            focused_widget,
            beskid_exe,
        }
    }
}
