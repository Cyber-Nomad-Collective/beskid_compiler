//! TEA update: apply messages to the model.

use crate::logging::clear_tui_log_buffer;

use super::message::Message;
use super::model::{Mode, Model};
use super::test_table::TestRowState;
use super::widgets::init_session_logger;

pub fn update(model: &mut Model, msg: Message) -> Option<Message> {
    match msg {
        Message::PhaseStart { depth, label } => {
            if depth == 0 {
                model.last_work_unit = None;
            }
            model.tree.phase_start(depth, &label);
            tracing::info!(target: "beskid.tools.pipeline.ui", depth, label = label.as_str(), "phase");
            expand_tree(model);
        }
        Message::PhaseEnd {
            depth,
            label,
            duration,
        } => {
            model.tree.phase_end(depth, &label, &duration);
            tracing::info!(
                target: "beskid.tools.pipeline.ui",
                depth,
                label = label.as_str(),
                duration = duration.as_str(),
                "phase done"
            );
            expand_tree(model);
        }
        Message::WorkUnit {
            depth,
            done,
            total,
            label,
        } => {
            model.last_work_unit = Some(format!("[{done}/{total}] {label}"));
            model.tree.work_unit(depth, done, total, &label);
            tracing::trace!(
                target: "beskid.tools.pipeline.ui",
                depth,
                done,
                total,
                label = label.as_str(),
                "work unit"
            );
            expand_tree(model);
        }
        Message::SetProgress {
            total_pos,
            total_len,
            total_label,
            stage_pos,
            stage_len,
            stage_label,
        } => {
            model.pipeline.total_pos = total_pos;
            model.pipeline.total_len = total_len.max(1);
            model.pipeline.total_label = total_label;
            model.pipeline.stage_pos = stage_pos;
            model.pipeline.stage_len = stage_len.max(1);
            model.pipeline.stage_label = stage_label;
        }
        Message::PushLog(line) => {
            tracing::info!(target: "beskid.tools.pipeline", "{line}");
        }
        Message::BeginTests { title, rows } => {
            init_session_logger();
            clear_tui_log_buffer();
            model.logger_state = tui_logger::TuiWidgetState::new();
            model.test_title = Some(title);
            model.test_rows = rows;
            model.tests_loaded = true;
        }
        Message::UpdateTestRows(rows) => {
            model.test_rows = rows;
        }
        Message::ShowTestReport { summary, title } => {
            init_session_logger();
            seed_failure_logs(&model.test_rows);
            model.report_summary = summary;
            model.test_title = Some(title.clone());
            model.command_summary = summary.into_command_summary(title);
            model.summary_ready = true;
        }
        Message::StageSummary(summary) => {
            model.command_summary = summary;
            model.summary_ready = true;
        }
        Message::ShowTestsScreen => {
            model.mode = Mode::Tests;
        }
        Message::ShowSummaryScreen => {
            model.mode = Mode::Summary;
        }
        Message::CompileComplete => {
            model.compile_complete = true;
        }
    }
    None
}

fn seed_failure_logs(rows: &[super::test_table::TestRow]) {
    for row in rows {
        if row.state == TestRowState::Failed {
            tracing::error!(target: "beskid.tools.test", name = row.qualified_name.as_str(), "FAIL");
        }
    }
}

fn expand_tree(model: &mut Model) {
    for path in model.tree.open_paths() {
        let key = path.join(".");
        if model.expanded_tree_paths.insert(key) {
            model.tree_state.open(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tree_opens_new_paths_once() {
        let mut model = Model::default();
        model.tree.phase_start(0, "Resolve");
        expand_tree(&mut model);
        let first = model.expanded_tree_paths.len();
        assert_eq!(first, 1);
        expand_tree(&mut model);
        assert_eq!(model.expanded_tree_paths.len(), first);
        model.tree.phase_start(1, "Graph");
        expand_tree(&mut model);
        assert_eq!(model.expanded_tree_paths.len(), first + 1);
    }
}
