//! Base compile shell: header, stage, detail tree, log, progress footer.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::logging::clear_tui_log_buffer;
use crate::pipeline::tui::log_tabs::{log_tab_for_phase_label, BUILD_LOG_TARGET};
use crate::pipeline::tui::stage_focus::StageFocus;
use crate::pipeline::tui::{TestRow, TestRowState};
use crate::pipeline::tui::widgets::{
    draw_context_bar, draw_pipeline_tree, draw_progress_footer, draw_stage_panel,
    draw_tabbed_log_panel, init_session_logger,
};
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::layout::{
    PANEL_DETAIL, PANEL_FOOTER, PANEL_HEADER, PANEL_LOG, PANEL_STAGE,
};
use crate::tui::shell::input;
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
    apply_pipeline_message(msg, state);
    Vec::new()
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_base_input(event, state)
}

pub fn render_panel(
    kind: &str,
    area: Rect,
    frame: &mut Frame,
    state: &mut ShellState,
) {
        let focus = StageFocus::from_shell_state(state);
        match kind {
            PANEL_HEADER => draw_context_bar(frame, area, state, focus),
            PANEL_STAGE => draw_stage_panel(frame, area, state, focus),
            PANEL_DETAIL => {
                let title = focus.title();
                draw_pipeline_tree(frame, area, &state.tree_nodes, &mut state.tree_state, title);
            }
            PANEL_LOG => {
                draw_tabbed_log_panel(frame, area, state.log_tab, &mut state.log_states);
            }
            PANEL_FOOTER => draw_progress_footer(frame, area, &state.pipeline),
            _ => {}
        }
}

pub fn apply_pipeline_message(msg: &ShellMessage, state: &mut ShellState) {
    match msg {
        ShellMessage::PhaseStart { depth, label } => {
            if *depth == 0 {
                state.last_work_unit = None;
            }
            if *depth <= 1 {
                state.log_tab = log_tab_for_phase_label(label);
            }
            state.tree.phase_start(*depth, label);
            trace_phase(label, *depth, None, "phase start");
            expand_tree(state);
        }
        ShellMessage::PhaseEnd {
            depth,
            label,
            duration,
        } => {
            state.tree.phase_end(*depth, label, duration);
            trace_phase(label, *depth, Some(duration.as_str()), "phase done");
            expand_tree(state);
        }
        ShellMessage::ActiveWork { done, total, label } => {
            state.last_work_unit = Some(format!("[{done}/{total}] {label}"));
        }
        ShellMessage::WorkUnit {
            depth,
            done,
            total,
            label,
        } => {
            state.tree.work_unit(*depth, *done, *total, label);
            tracing::trace!(
                target: BUILD_LOG_TARGET,
                depth,
                done,
                total,
                label = label.as_str(),
                "work unit"
            );
            expand_tree(state);
        }
        ShellMessage::SetProgress {
            total_pos,
            total_len,
            total_label,
            stage_pos,
            stage_len,
            stage_label,
        } => {
            state.pipeline.total_pos = *total_pos;
            state.pipeline.total_len = (*total_len).max(1);
            state.pipeline.total_label.clone_from(total_label);
            state.pipeline.stage_pos = *stage_pos;
            state.pipeline.stage_len = (*stage_len).max(1);
            state.pipeline.stage_label.clone_from(stage_label);
        }
        ShellMessage::PushLog(line) => {
            tracing::info!(target: BUILD_LOG_TARGET, "{line}");
        }
        ShellMessage::BeginTests { title, rows } => {
            init_session_logger();
            clear_tui_log_buffer();
            state.log_states.reset();
            state.test_title = Some(title.clone());
            state.test_rows.clone_from(rows);
            state.tests_loaded = true;
            state.sync_code_viewer_for_selection();
        }
        ShellMessage::UpdateTestRows(rows) => {
            state.test_rows.clone_from(rows);
            state.sync_code_viewer_for_selection();
        }
        ShellMessage::ShowTestReport { summary, title } => {
            init_session_logger();
            seed_failure_logs(&state.test_rows);
            state.report_summary = *summary;
            state.test_title = Some(title.clone());
            state.command_summary = summary.into_command_summary(title.clone());
            state.summary_ready = true;
        }
        ShellMessage::StageSummary(summary) => {
            state.command_summary = summary.clone();
            state.summary_ready = true;
        }
        ShellMessage::CompileComplete => {
            state.compile_complete = true;
        }
        ShellMessage::SetOverlayVisible { kind, visible } => {
            state.set_overlay_visible(*kind, *visible);
            if *visible {
                state.focus_overlay(*kind);
            }
        }
        ShellMessage::FocusOverlay(kind) => {
            state.focus_overlay(*kind);
        }
        ShellMessage::FocusBase => {
            state.focus_base(state.pane_focus);
        }
        ShellMessage::Tick => {
            state.tick = state.tick.wrapping_add(1);
        }
        ShellMessage::PckgCatalogLoaded(_)
        | ShellMessage::PckgCatalogFailed(_)
        | ShellMessage::PckgDetailsLoaded(_)
        | ShellMessage::PckgDetailsFailed(_)
        | ShellMessage::TemplatesLoaded { .. }
        | ShellMessage::TemplatesLoadFailed(_)
        | ShellMessage::TemplateInstallDone { .. }
        | ShellMessage::TemplateInstallFailed { .. }
        | ShellMessage::EnterProjectWizard => {}
    }
}

fn trace_phase(label: &str, depth: usize, duration: Option<&str>, message: &'static str) {
    let tab = log_tab_for_phase_label(label);
    match (tab, duration) {
        (crate::pipeline::tui::log_tabs::LogTab::Semantic, Some(duration)) => {
            tracing::info!(
                target: "beskid_tools::pipeline::semantic",
                depth,
                label,
                duration,
                "{message}"
            );
        }
        (crate::pipeline::tui::log_tabs::LogTab::Semantic, None) => {
            tracing::info!(
                target: "beskid_tools::pipeline::semantic",
                depth,
                label,
                "{message}"
            );
        }
        (_, Some(duration)) => {
            tracing::info!(
                target: "beskid_tools::pipeline::build",
                depth,
                label,
                duration,
                "{message}"
            );
        }
        (_, None) => {
            tracing::info!(
                target: "beskid_tools::pipeline::build",
                depth,
                label,
                "{message}"
            );
        }
    }
}

fn seed_failure_logs(rows: &[TestRow]) {
    for row in rows {
        if row.state == TestRowState::Failed {
            tracing::error!(target: "beskid.tools.test", name = row.qualified_name.as_str(), "FAIL");
        }
    }
}

fn expand_tree(state: &mut ShellState) {
    state.tree_nodes = state.tree.tree_nodes();
    for path in state.tree.open_paths() {
        let key = path
            .iter()
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(".");
        if state.expanded_tree_paths.insert(key) {
            state.tree_state.expand(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tree_opens_new_paths_once() {
        let mut state = ShellState::default();
        state.tree.phase_start(0, "Resolve");
        expand_tree(&mut state);
        let first = state.expanded_tree_paths.len();
        assert_eq!(first, 1);
        expand_tree(&mut state);
        assert_eq!(state.expanded_tree_paths.len(), first);
        state.tree.phase_start(1, "Graph");
        expand_tree(&mut state);
        assert_eq!(state.expanded_tree_paths.len(), first + 1);
    }
}
