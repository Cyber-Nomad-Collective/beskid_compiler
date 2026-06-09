//! Shared shell state: pipeline progress, overlays, cached layout rects.

use std::collections::HashSet;

use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use ratkit::widgets::{TreeNavigator, TreeNode, TreeViewState};

use crate::pipeline::tui::log_tabs::{LogTab, LogTabStates};
use crate::pipeline::tui::{
    CommandSummary, PipelineProgress, PipelineTree, TestReportSummary, TestRow, TestRowState,
};
use crate::tui::widgets::CodeViewerPanel;

use super::focus::{FocusTarget, OverlayKind, PaneFocus};
use super::pane_state::{PckgPaneState, ShellMode, TemplatesPaneState};

/// Which sub-panel has focus inside an overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayPanelFocus {
    #[default]
    List,
    Code,
}

/// Cached panel/overlay rectangles from the last panes resolve (mouse hit-testing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutRects {
    pub header: Rect,
    pub stage: Rect,
    pub detail: Rect,
    pub log: Rect,
    pub footer: Rect,
    pub tests_overlay: Option<Rect>,
    pub summary_overlay: Option<Rect>,
    pub pckg_overlay: Option<Rect>,
    pub templates_overlay: Option<Rect>,
}

/// Cross-thread navigation wait targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    Tests,
    Summary,
    Exit,
}

/// Application state shared across screens.
pub struct ShellState {
    pub tick: u64,
    pub focus: FocusTarget,
    pub pane_focus: PaneFocus,
    pub layout_rects: LayoutRects,
    pub pipeline: PipelineProgress,
    pub tree: PipelineTree,
    pub tree_nodes: Vec<TreeNode<String>>,
    pub tree_state: TreeViewState,
    pub tree_navigator: TreeNavigator,
    pub last_work_unit: Option<String>,
    pub log_tab: LogTab,
    pub log_states: LogTabStates,
    pub expanded_tree_paths: HashSet<String>,
    pub compile_complete: bool,
    pub tests_loaded: bool,
    pub summary_ready: bool,
    pub awaiting_nav: Option<NavTarget>,
    pub overlay_visible: [bool; 4],
    pub test_rows: Vec<TestRow>,
    pub test_title: Option<String>,
    pub test_list_state: ListState,
    pub test_list_user_selected: bool,
    pub report_summary: TestReportSummary,
    pub command_summary: CommandSummary,
    pub code_viewer: CodeViewerPanel,
    pub overlay_panel_focus: OverlayPanelFocus,
    pub summary_explorer_index: usize,
    pub quit_requested: bool,
    pub shell_mode: ShellMode,
    pub pckg: PckgPaneState,
    pub templates: TemplatesPaneState,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            tick: 0,
            focus: FocusTarget::default(),
            pane_focus: PaneFocus::default(),
            layout_rects: LayoutRects::default(),
            pipeline: PipelineProgress::default(),
            tree: PipelineTree::default(),
            tree_nodes: Vec::new(),
            tree_state: TreeViewState::new(),
            tree_navigator: TreeNavigator::new(),
            last_work_unit: None,
            log_tab: LogTab::default(),
            log_states: LogTabStates::default(),
            expanded_tree_paths: HashSet::new(),
            compile_complete: false,
            tests_loaded: false,
            summary_ready: false,
            awaiting_nav: None,
            overlay_visible: [false; 4],
            test_rows: Vec::new(),
            test_title: None,
            test_list_state: ListState::default(),
            test_list_user_selected: false,
            report_summary: TestReportSummary::default(),
            command_summary: CommandSummary::default(),
            code_viewer: CodeViewerPanel::default(),
            overlay_panel_focus: OverlayPanelFocus::default(),
            summary_explorer_index: 0,
            quit_requested: false,
            shell_mode: ShellMode::default(),
            pckg: PckgPaneState::default(),
            templates: TemplatesPaneState {
                registry_config: crate::tui::panes::template_ops::default_registry_config(),
                ..TemplatesPaneState::default()
            },
        }
    }
}

impl ShellState {
    pub fn failed_test_indices(&self) -> Vec<usize> {
        self.test_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                if row.state == TestRowState::Failed {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn sync_code_viewer_for_test(&mut self, index: usize) {
        let Some(row) = self.test_rows.get(index) else {
            self.code_viewer.clear();
            return;
        };
        if let Some(link) = row.link.as_ref()
            && self.code_viewer.load_file(&link.path, Some(link.line)).is_ok()
        {
            return;
        }
        if let Some(detail) = row.failure_detail.as_deref() {
            self.code_viewer
                .load_text(&row.qualified_name, detail, "text");
        } else {
            self.code_viewer.clear();
        }
    }

    pub fn sync_code_viewer_for_selection(&mut self) {
        let index = self
            .test_list_state
            .selected()
            .or_else(|| selected_running_index(&self.test_rows));
        if let Some(index) = index {
            self.sync_code_viewer_for_test(index);
        }
    }

    pub fn sync_pckg_detail_viewer(&mut self) {
        let Some(detail) = self.pckg.detail.as_ref() else {
            self.code_viewer.clear();
            return;
        };
        let readme = detail
            .readme
            .as_deref()
            .unwrap_or("No readme published for this package.");
        self.code_viewer
            .load_text(&detail.package.name, readme, "markdown");
    }

    pub fn sync_template_detail_viewer(&mut self) {
        let Some(index) = self.templates.list_state.selected() else {
            self.code_viewer.clear();
            return;
        };
        let text = match self.templates.tab {
            super::pane_state::TemplateListTab::Installed => self
                .templates
                .installed
                .get(index)
                .map(|row| {
                    format!(
                        "shortName: {}\nname: {}\npackage: {}\nversion: {}\n\nInstalled template ready to scaffold with `beskid new {} -o <dir>`.",
                        row.short_name,
                        row.name,
                        row.package_id.as_deref().unwrap_or("—"),
                        row.version.as_deref().unwrap_or("—"),
                        row.short_name
                    )
                }),
            super::pane_state::TemplateListTab::Registry => self
                .templates
                .registry
                .get(index)
                .map(|row| {
                    format!(
                        "package: {}\n\n{}\n\nPress i or Enter to download and install this template into the local cache.",
                        row.package_id, row.description
                    )
                }),
        };
        if let Some(text) = text {
            self.code_viewer.load_text("template", &text, "text");
        } else {
            self.code_viewer.clear();
        }
    }

    pub fn sync_summary_explorer(&mut self) {
        let failed = self.failed_test_indices();
        if failed.is_empty() {
            self.code_viewer.clear();
            return;
        }
        if self.summary_explorer_index >= failed.len() {
            self.summary_explorer_index = failed.len().saturating_sub(1);
        }
        let row_index = failed[self.summary_explorer_index];
        self.test_list_state.select(Some(row_index));
        self.sync_code_viewer_for_test(row_index);
    }
}

fn selected_running_index(rows: &[TestRow]) -> Option<usize> {
    rows.iter()
        .position(|row| row.state == TestRowState::Running)
        .or_else(|| rows.iter().rposition(|row| row.state != TestRowState::Pending))
}

impl ShellState {
    pub fn show_spinner(&self) -> bool {
        !self.compile_complete && self.focus.is_base()
    }

    pub fn overlay_visible(&self, kind: OverlayKind) -> bool {
        self.overlay_visible[overlay_index(kind)]
    }

    pub fn set_overlay_visible(&mut self, kind: OverlayKind, visible: bool) {
        self.overlay_visible[overlay_index(kind)] = visible;
    }

    pub fn focus_overlay(&mut self, kind: OverlayKind) {
        self.focus = FocusTarget::Overlay(kind);
    }

    pub fn focus_base(&mut self, pane: PaneFocus) {
        self.pane_focus = pane;
        self.focus = FocusTarget::Base(pane);
    }

    pub fn close_focused_overlay(&mut self) {
        if let FocusTarget::Overlay(kind) = self.focus {
            self.set_overlay_visible(kind, false);
            self.focus_base(PaneFocus::Stage);
        }
    }

    pub fn navigation_hint(&self) -> Option<&'static str> {
        if self.focus.is_base()
            && self.compile_complete
            && self.awaiting_nav == Some(NavTarget::Tests)
            && self.tests_loaded
        {
            return Some("[Space] tests");
        }
        if self.focus.is_base() && self.tests_loaded && !self.overlay_visible(OverlayKind::Tests) {
            return Some("[Space] tests");
        }
        if self.focus.is_base() && self.compile_complete {
            return Some("Compile complete");
        }
        if self.focus == FocusTarget::Overlay(OverlayKind::Tests) && self.summary_ready {
            return Some("[Space] summary");
        }
        if self.focus == FocusTarget::Overlay(OverlayKind::Summary) {
            return Some("[Space/q] exit");
        }
        if self.focus.is_base() && self.summary_ready {
            return Some("[Space] summary");
        }
        None
    }

    pub fn advance_once(&mut self) -> Option<NavTarget> {
        if self.focus.is_base() && self.tests_loaded {
            self.set_overlay_visible(OverlayKind::Tests, true);
            self.focus_overlay(OverlayKind::Tests);
            self.sync_code_viewer_for_selection();
            return None;
        }
        if self.focus == FocusTarget::Overlay(OverlayKind::Tests) && self.summary_ready {
            self.set_overlay_visible(OverlayKind::Summary, true);
            self.focus_overlay(OverlayKind::Summary);
            self.sync_summary_explorer();
            return None;
        }
        if self.focus.is_base() && self.summary_ready {
            self.set_overlay_visible(OverlayKind::Summary, true);
            self.focus_overlay(OverlayKind::Summary);
            self.sync_summary_explorer();
            return None;
        }
        if self.focus == FocusTarget::Overlay(OverlayKind::Summary) {
            return Some(NavTarget::Exit);
        }
        None
    }

    pub fn nav_reached(&self, target: NavTarget) -> bool {
        match target {
            NavTarget::Tests => {
                self.overlay_visible(OverlayKind::Tests)
                    && self.focus == FocusTarget::Overlay(OverlayKind::Tests)
            }
            NavTarget::Summary => {
                self.overlay_visible(OverlayKind::Summary)
                    && self.focus == FocusTarget::Overlay(OverlayKind::Summary)
            }
            NavTarget::Exit => false,
        }
    }
}

fn overlay_index(kind: OverlayKind) -> usize {
    match kind {
        OverlayKind::Tests => 0,
        OverlayKind::Summary => 1,
        OverlayKind::Pckg => 2,
        OverlayKind::Templates => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::shell::focus::FocusTarget;

    #[test]
    fn advance_opens_tests_overlay() {
        let mut state = ShellState {
            tests_loaded: true,
            compile_complete: true,
            ..Default::default()
        };
        assert!(state.advance_once().is_none());
        assert!(state.overlay_visible(OverlayKind::Tests));
        assert_eq!(state.focus, FocusTarget::Overlay(OverlayKind::Tests));
    }

    #[test]
    fn nav_reached_when_tests_overlay_focused() {
        let mut state = ShellState::default();
        state.set_overlay_visible(OverlayKind::Tests, true);
        state.focus_overlay(OverlayKind::Tests);
        assert!(state.nav_reached(NavTarget::Tests));
    }

    #[test]
    fn advance_tests_then_summary_then_exit() {
        let mut state = ShellState {
            tests_loaded: true,
            compile_complete: true,
            awaiting_nav: Some(NavTarget::Tests),
            ..Default::default()
        };
        assert!(state.navigation_hint().is_some());
        assert!(state.advance_once().is_none());
        assert!(state.overlay_visible(OverlayKind::Tests));
        assert_eq!(state.focus, FocusTarget::Overlay(OverlayKind::Tests));

        state.summary_ready = true;
        assert!(state.advance_once().is_none());
        assert!(state.overlay_visible(OverlayKind::Summary));
        assert_eq!(state.focus, FocusTarget::Overlay(OverlayKind::Summary));

        assert_eq!(state.advance_once(), Some(NavTarget::Exit));
    }
}
