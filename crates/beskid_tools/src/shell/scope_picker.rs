//! Workspace/project picker overlay (ratatui-explorer).

use std::path::Path;

use ratatui::Frame;
use ratatui::widgets::FrameExt;
use ratatui::layout::Rect;
use ratatui_explorer::{File, FileExplorer, Input, Theme};

use crate::shell::scope::ShellScope;
use crate::tui::overlay_chrome::{draw_backdrop, render_overlay_panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePickerMode {
    Workspace,
    Project,
}

pub struct ScopePickerOverlay {
    pub mode: ScopePickerMode,
    explorer: FileExplorer,
}

impl ScopePickerOverlay {
    pub fn open(mode: ScopePickerMode) -> Result<Self, Box<dyn std::error::Error>> {
        let mut explorer = FileExplorer::new()?;
        let filter_mode = mode;
        explorer.set_filter_map(move |file: File| filter_entry(file, filter_mode))?;
        explorer.set_theme(Theme::default().add_default_title());
        Ok(Self { mode, explorer })
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ScopePickerAction {
        use crossterm::event::{KeyCode, KeyModifiers};
        match key.code {
            KeyCode::Esc => ScopePickerAction::Close,
            KeyCode::Enter => {
                let path = self.explorer.current().path.clone();
                ScopePickerAction::Selected(path)
            }
            KeyCode::Up => {
                let _ = self.explorer.handle(Input::Up);
                ScopePickerAction::Redraw
            }
            KeyCode::Down => {
                let _ = self.explorer.handle(Input::Down);
                ScopePickerAction::Redraw
            }
            KeyCode::Left => {
                let _ = self.explorer.handle(Input::Left);
                ScopePickerAction::Redraw
            }
            KeyCode::Right => {
                let _ = self.explorer.handle(Input::Right);
                ScopePickerAction::Redraw
            }
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let show = !self.explorer.show_hidden();
                let _ = self.explorer.set_show_hidden(show);
                ScopePickerAction::Redraw
            }
            _ => ScopePickerAction::Redraw,
        }
    }

    pub fn render(&self, area: Rect, frame: &mut Frame) {
        draw_backdrop(frame, area);
        let title = match self.mode {
            ScopePickerMode::Workspace => "Open workspace (.bws)",
            ScopePickerMode::Project => "Open project (.bproj)",
        };
        render_overlay_panel(frame, area, title, &[], |body, f| {
            f.render_widget_ref(self.explorer.widget(), body);
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopePickerAction {
    Redraw,
    Close,
    Selected(std::path::PathBuf),
}

pub fn resolve_picked_scope(path: &Path) -> ShellScope {
    ShellScope::resolve(path)
}

fn filter_entry(file: File, mode: ScopePickerMode) -> Option<File> {
    if file.is_dir {
        return Some(file);
    }
    let ext = file.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match mode {
        ScopePickerMode::Workspace if ext == "bws" => Some(file),
        ScopePickerMode::Project if ext == "bproj" => Some(file),
        _ => None,
    }
}
