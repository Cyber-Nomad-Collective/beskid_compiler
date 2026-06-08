//! TEA messages: every UI state transition.

use super::model::{CommandSummary, TestReportSummary};
use super::test_table::TestRow;

#[derive(Debug, Clone)]
pub enum Message {
    PhaseStart {
        depth: usize,
        label: String,
    },
    PhaseEnd {
        depth: usize,
        label: String,
        duration: String,
    },
    WorkUnit {
        depth: usize,
        done: u64,
        total: u64,
        label: String,
    },
    SetProgress {
        total_pos: u64,
        total_len: u64,
        total_label: String,
        stage_pos: u64,
        stage_len: u64,
        stage_label: String,
    },
    PushLog(String),
    BeginTests {
        title: String,
        rows: Vec<TestRow>,
    },
    UpdateTestRows(Vec<TestRow>),
    ShowTestReport {
        summary: TestReportSummary,
        title: String,
    },
    ShowSummary(CommandSummary),
}
