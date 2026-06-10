//! Shell phase / screen-transition labels for the pinned top bar.

use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::pane_state::ShellMode;
use crate::tui::shell::state::ShellState;

/// Human-readable workflow position (compile → tests → summary, or page title).
pub fn transition_label(state: &ShellState, page_title: &str) -> String {
    if state.overlay_visible(OverlayKind::Summary) {
        let failed = state.failed_test_indices().len();
        let total = state.test_rows.len();
        if failed > 0 {
            return format!("Summary · {failed} failed / {total}");
        }
        return format!("Summary · {total} passed");
    }
    if state.overlay_visible(OverlayKind::Tests) {
        let running = state
            .test_rows
            .iter()
            .filter(|r| {
                r.state == crate::pipeline::tui::TestRowState::Running
                    || r.state == crate::pipeline::tui::TestRowState::Pending
            })
            .count();
        let done = state.test_rows.len().saturating_sub(running);
        if running > 0 {
            return format!("Tests · {done}/{} complete", state.test_rows.len());
        }
        return format!("Tests · {} cases", state.test_rows.len());
    }
    let compiling = if state.shell_mode == ShellMode::Hi {
        !state.compile_complete && state.pipeline_active()
    } else {
        !state.compile_complete
    };
    if compiling {
        let stage = state.pipeline.stage_label.trim();
        if stage.is_empty() {
            return "Compiling…".into();
        }
        return format!("Compiling · {stage}");
    }
    if state.summary_ready {
        return "Complete · press Space for summary".into();
    }
    if state.tests_loaded {
        return "Tests ready · press Space".into();
    }
    if let Some(hint) = state.navigation_hint() {
        return hint.to_string();
    }
    page_title.to_string()
}
