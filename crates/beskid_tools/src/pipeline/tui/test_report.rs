//! Test report helpers (re-export model types; seed failure logs).

pub use super::model::TestReportSummary;
use super::test_table::{TestRow, TestRowState};

/// Push failure details into the tui-logger buffer for the report panel.
pub fn seed_failure_logs(rows: &[TestRow]) {
    for row in rows {
        if row.state == TestRowState::Failed {
            tracing::error!(target: "beskid.tools.test", name = row.qualified_name.as_str(), "FAIL");
        }
    }
}

pub fn init_test_logger() {
    super::widgets::init_session_logger();
}
