//! Shared pipeline UI value types (progress bars, summaries, test reports).
//!
//! All interactive state lives in [`crate::tui::shell::state::ShellState`].

use ratatui::style::Color;

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

/// Outcome counts for test runs (converts to [`CommandSummary`]).
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
