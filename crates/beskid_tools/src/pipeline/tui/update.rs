//! TEA update: apply messages to the model.

use super::message::Message;
use super::model::{Mode, Model};
use super::widgets::init_session_logger;
use super::test_table::TestRowState;

pub fn update(model: &mut Model, msg: Message) -> Option<Message> {
    match msg {
        Message::PhaseStart { depth, label } => {
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
            model.mode = Mode::Tests;
            model.test_title = Some(title);
            model.test_rows = rows;
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
            model.mode = Mode::Report;
        }
        Message::ShowSummary(summary) => {
            init_session_logger();
            model.command_summary = summary;
            model.mode = Mode::Summary;
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
    if let Ok(items) = model.tree.tree_items() {
        for item in items {
            model.tree_state.open(vec![item.identifier().clone()]);
        }
    }
    for path in model.tree.open_paths() {
        model.tree_state.open(path);
    }
}
