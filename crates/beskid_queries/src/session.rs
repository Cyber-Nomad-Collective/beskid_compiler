//! Process-scoped `BeskidDatabase` for CLI and other callers without a long-lived session handle.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{PrepareOptions, PreparedCompilation, ResolvedInput};
use beskid_pipeline::PipelineObserver;

use crate::db::BeskidDatabase;
use crate::entry::{
    prepare_compilation_diagnostics_with_db, prepare_compilation_with_db,
};

thread_local! {
    static COMPILATION_DB: RefCell<BeskidDatabase> = RefCell::new(BeskidDatabase::default());
    static CONFIGURED_ROOT: RefCell<Option<PathBuf>> = RefCell::new(None);
}

/// Access the shared compilation database for this thread.
pub fn with_db<T>(f: impl FnOnce(&mut BeskidDatabase) -> T) -> T {
    COMPILATION_DB.with(|db| f(&mut db.borrow_mut()))
}

/// Configure on-disk persistence when the project root changes.
pub fn configure_db_for_project(project_root: &Path) {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    CONFIGURED_ROOT.with(|configured| {
        if configured.borrow().as_ref() == Some(&canonical) {
            return;
        }
        COMPILATION_DB.with(|db| {
            *db.borrow_mut() = BeskidDatabase::with_persistence(&canonical);
        });
        *configured.borrow_mut() = Some(canonical);
    });
}

fn ensure_db_for_resolved(resolved: &ResolvedInput) {
    if let Some(plan) = resolved.compile_plan.as_ref() {
        configure_db_for_project(&plan.project_root);
    }
}

/// Run the prepare spine with the process-scoped database (CLI / one-shot callers).
pub fn prepare_compilation_diagnostics(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>)> {
    with_db(|db| {
        ensure_db_for_resolved(resolved);
        prepare_compilation_diagnostics_with_db(db, resolved, options, pipeline)
    })
}

/// Run executable prepare with the process-scoped database (CLI / one-shot callers).
pub fn prepare_compilation(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    with_db(|db| {
        ensure_db_for_resolved(resolved);
        prepare_compilation_with_db(db, resolved, options, pipeline)
    })
}
