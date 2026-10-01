//! Values for line-oriented end-of-command summaries.

/// One key/value row in the summary.
#[derive(Debug, Clone)]
pub struct SummaryStat {
    pub label: String,
    pub value: String,
}

/// Generic end-of-command summary (build, analyze, …).
#[derive(Debug, Clone, Default)]
pub struct CommandSummary {
    pub title: String,
    pub headline: String,
    pub stats: Vec<SummaryStat>,
}

impl CommandSummary {
    pub fn plain(title: impl Into<String>, headline: impl Into<String>) -> Self {
        Self { title: title.into(), headline: headline.into(), stats: Vec::new() }
    }

    pub fn with_stat(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.stats.push(SummaryStat { label: label.into(), value: value.into() });
        self
    }
}
