//! Cross-thread messages for the unified shell.

use beskid_pckg::models::{PackageDetailsResponse, PackageSummaryResponse};

use crate::pipeline::tui::{CommandSummary, TestReportSummary, TestRow};
use crate::tui::panes::{InstalledTemplateView, RegistryTemplateView};
use crate::tui::shell::focus::OverlayKind;

#[derive(Debug, Clone)]
pub enum ShellMessage {
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
    ActiveWork {
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
    StageSummary(CommandSummary),
    CompileComplete,
    SetOverlayVisible {
        kind: OverlayKind,
        visible: bool,
    },
    FocusOverlay(OverlayKind),
    FocusBase,
    Tick,
    PckgCatalogLoaded(Vec<PackageSummaryResponse>),
    PckgCatalogFailed(String),
    PckgDetailsLoaded(Box<PackageDetailsResponse>),
    PckgDetailsFailed(String),
    TemplatesLoaded {
        installed: Vec<InstalledTemplateView>,
        registry: Vec<RegistryTemplateView>,
    },
    TemplatesLoadFailed(String),
    TemplateInstallDone {
        short_name: String,
        package_id: String,
    },
    TemplateInstallFailed {
        package_id: String,
        error: String,
    },
    EnterProjectWizard,
}
