//! TEA model: all pipeline UI state.

use std::collections::HashSet;

use ratatui::style::Color;
use ratatui::widgets::ListState;
use tui_logger::TuiWidgetState;
use tui_tree_widget::TreeState;

use super::pipeline_tree::PipelineTree;
use super::test_table::TestRow;

/// Dual progress-bar state pinned to the layout footer.
#[derive(Debug, Clone)]
pub struct PipelineProgress {
    pub total_pos: u64,
    pub total_len: u64,
    pub total_label: String,
    pub stage_pos: u64,
    pub stage_len: u64,
    pub stage_label: String,
}

impl Default for PipelineProgress {
    fn default() -> Self {
        Self {
            total_pos: 0,
            total_len: 1,
            total_label: "Pipeline".into(),
            stage_pos: 0,
            stage_len: 1,
            stage_label: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pipeline,
    Tests,
    Report,
    Summary,
}

/// One key/value row in the summary table.
#[derive(Debug, Clone)]
pub struct SummaryStat {
    pub label: String,
    pub value: String,
    pub color: Option<Color>,
}

/// Optional pie segment for summary charts.
#[derive(Debug, Clone)]
pub struct SummarySlice {
    pub label: String,
    pub percent: f64,
    pub color: Color,
}

/// Generic end-of-command summary (tests, build, analyze, …).
#[derive(Debug, Clone, Default)]
pub struct CommandSummary {
    pub title: String,
    pub headline: String,
    pub stats: Vec<SummaryStat>,
    pub slices: Vec<SummarySlice>,
}

impl CommandSummary {
    pub fn plain(title: impl Into<String>, headline: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            headline: headline.into(),
            stats: Vec::new(),
            slices: Vec::new(),
        }
    }

    pub fn with_stat(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.stats.push(SummaryStat {
            label: label.into(),
            value: value.into(),
            color: None,
        });
        self
    }
}

/// Outcome counts for test runs (legacy; converts to [`CommandSummary`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct TestReportSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub filtered_out: usize,
}

impl TestReportSummary {
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.skipped + self.filtered_out
    }

    pub fn into_command_summary(self, title: impl Into<String>) -> CommandSummary {
        let title = title.into();
        let total = self.total().max(1) as f64;
        let mut slices = Vec::new();
        if self.passed > 0 {
            slices.push(SummarySlice {
                label: "pass".into(),
                percent: self.passed as f64 * 100.0 / total,
                color: Color::Green,
            });
        }
        if self.failed > 0 {
            slices.push(SummarySlice {
                label: "fail".into(),
                percent: self.failed as f64 * 100.0 / total,
                color: Color::Red,
            });
        }
        if self.skipped > 0 {
            slices.push(SummarySlice {
                label: "skip".into(),
                percent: self.skipped as f64 * 100.0 / total,
                color: Color::Blue,
            });
        }
        if self.filtered_out > 0 {
            slices.push(SummarySlice {
                label: "filt".into(),
                percent: self.filtered_out as f64 * 100.0 / total,
                color: Color::DarkGray,
            });
        }
        if slices.is_empty() {
            slices.push(SummarySlice {
                label: "empty".into(),
                percent: 100.0,
                color: Color::DarkGray,
            });
        }
        CommandSummary {
            title: title.clone(),
            headline: format!(
                "passed={} failed={} skipped={} filtered={}",
                self.passed, self.failed, self.skipped, self.filtered_out
            ),
            stats: vec![
                SummaryStat {
                    label: "passed".into(),
                    value: self.passed.to_string(),
                    color: Some(Color::Green),
                },
                SummaryStat {
                    label: "failed".into(),
                    value: self.failed.to_string(),
                    color: Some(Color::Red),
                },
                SummaryStat {
                    label: "skipped".into(),
                    value: self.skipped.to_string(),
                    color: Some(Color::Blue),
                },
                SummaryStat {
                    label: "filtered".into(),
                    value: self.filtered_out.to_string(),
                    color: Some(Color::DarkGray),
                },
            ],
            slices,
        }
    }
}

/// Application model: tree, log buffer, progress, and mode-specific panels.
pub struct Model {
    pub mode: Mode,
    pub pipeline: PipelineProgress,
    pub tree: PipelineTree,
    pub tree_state: TreeState<String>,
    pub last_work_unit: Option<String>,
    pub test_rows: Vec<TestRow>,
    pub test_title: Option<String>,
    pub test_list_state: ListState,
    pub report_summary: TestReportSummary,
    pub command_summary: CommandSummary,
    pub logger_state: TuiWidgetState,
    /// Tree node paths already passed to [`TreeState::open`](tui_tree_widget::TreeState::open).
    pub expanded_tree_paths: HashSet<String>,
    /// Compile/prepare finished; pipeline screen may remain until Space.
    pub compile_complete: bool,
    /// Test rows loaded; Space can open the test screen.
    pub tests_loaded: bool,
    /// Outcome summary staged; Space can open the summary screen.
    pub summary_ready: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            mode: Mode::Pipeline,
            pipeline: PipelineProgress::default(),
            tree: PipelineTree::default(),
            tree_state: TreeState::default(),
            last_work_unit: None,
            test_rows: Vec::new(),
            test_title: None,
            test_list_state: ListState::default(),
            report_summary: TestReportSummary::default(),
            command_summary: CommandSummary::default(),
            logger_state: TuiWidgetState::new(),
            expanded_tree_paths: HashSet::new(),
            compile_complete: false,
            tests_loaded: false,
            summary_ready: false,
        }
    }
}
