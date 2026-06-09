//! Build vs semantic log streams (filtered via per-tab [`TuiWidgetState`] level config).

use log::LevelFilter;
use tui_logger::TuiWidgetState;

pub const BUILD_LOG_TARGET: &str = "beskid_tools::pipeline::build";
pub const SEMANTIC_LOG_TARGET: &str = "beskid_tools::pipeline::semantic";

/// Which filtered view of the shared tui-logger buffer to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogTab {
    #[default]
    Build,
    Semantic,
}

impl LogTab {
    pub const ALL: [LogTab; 2] = [LogTab::Build, LogTab::Semantic];

    pub fn index(self) -> usize {
        match self {
            Self::Build => 0,
            Self::Semantic => 1,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Build => "Build",
            Self::Semantic => "Semantic",
        }
    }

    pub fn scroll_hint(self) -> &'static str {
        match self {
            Self::Build => "Build log (↑↓ scroll · End tail)",
            Self::Semantic => "Semantic log (↑↓ scroll · End tail)",
        }
    }
}

/// Per-tab scroll/filter state over one shared logger buffer.
pub struct LogTabStates {
    states: [TuiWidgetState; 2],
}

impl Default for LogTabStates {
    fn default() -> Self {
        Self::new()
    }
}

impl LogTabStates {
    pub fn new() -> Self {
        Self {
            states: [configured_state(LogTab::Build), configured_state(LogTab::Semantic)],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn state_mut(&mut self, tab: LogTab) -> &mut TuiWidgetState {
        &mut self.states[tab.index()]
    }
}

fn configured_state(tab: LogTab) -> TuiWidgetState {
    let state = TuiWidgetState::new().set_default_display_level(LevelFilter::Trace);
    match tab {
        LogTab::Build => state
            .set_level_for_target(SEMANTIC_LOG_TARGET, LevelFilter::Off)
            .set_level_for_target("beskid_analysis", LevelFilter::Trace),
        LogTab::Semantic => state
            .set_level_for_target(BUILD_LOG_TARGET, LevelFilter::Off)
            .set_level_for_target("beskid_tools::test", LevelFilter::Off)
            .set_level_for_target("beskid.pipeline", LevelFilter::Off)
            .set_level_for_target("beskid.tools.pipeline", LevelFilter::Off)
            .set_level_for_target("beskid.tools.pipeline.ui", LevelFilter::Off),
    }
}

/// Classify a human phase label for tracing (build vs semantic log tab).
pub fn log_tab_for_phase_label(label: &str) -> LogTab {
    let lower = label.to_ascii_lowercase();
    if lower.contains("semantic")
        || lower.contains("lower ast")
        || lower.contains("collect definition")
        || lower.contains("control flow")
        || lower.contains("resolve name")
        || lower.contains("visibility")
        || lower.contains("contract")
        || lower.contains("error handling")
        || (lower.contains("type check") && !lower.contains("jit"))
        || lower.contains("normalize hir")
        || lower.contains("resolve (pass")
    {
        LogTab::Semantic
    } else {
        LogTab::Build
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_labels_route_to_semantic_tab() {
        assert_eq!(
            log_tab_for_phase_label("Semantic analysis"),
            LogTab::Semantic
        );
        assert_eq!(log_tab_for_phase_label("Resolve manifest"), LogTab::Build);
    }
}
