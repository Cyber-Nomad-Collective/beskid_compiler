//! Process-scoped `BeskidDatabase` for CLI and other callers without a long-lived session handle.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, PrepareOptions, PreparedCompilation, ResolvedInput,
};
use beskid_pipeline::PipelineObserver;

use crate::db::{BeskidDatabase, configure_compilation_database_for_project};
use crate::entry::{prepare_compilation_diagnostics_with_db, prepare_compilation_with_db};

thread_local! {
    static COMPILATION_DB: RefCell<BeskidDatabase> = RefCell::new(BeskidDatabase::default());
    static CONFIGURED_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Access the shared compilation database for this thread.
pub fn with_db<T>(f: impl FnOnce(&mut BeskidDatabase) -> T) -> T {
    COMPILATION_DB.with(|db| f(&mut db.borrow_mut()))
}

fn configure_db_in_place(db: &mut BeskidDatabase, project_root: &Path) {
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
    let already_configured = CONFIGURED_ROOT.with(|configured| configured.borrow().as_ref() == Some(&canonical));
    if already_configured {
        return;
    }
    configure_compilation_database_for_project(db, &canonical);
    CONFIGURED_ROOT.with(|configured| {
        *configured.borrow_mut() = Some(canonical);
    });
}

/// Configure on-disk persistence when the project root changes.
pub fn configure_db_for_project(project_root: &Path) {
    with_db(|db| configure_db_in_place(db, project_root));
}

fn ensure_db_for_resolved(db: &mut BeskidDatabase, resolved: &ResolvedInput) {
    if let Some(plan) = resolved.compile_plan.as_ref() {
        configure_db_in_place(db, &plan.project_root);
    }
}

/// Run the prepare spine with the process-scoped database (CLI / one-shot callers).
pub fn prepare_compilation_diagnostics(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<beskid_analysis::SyntaxFix>)> {
    let result = with_db(|db| {
        ensure_db_for_resolved(db, resolved);
        prepare_compilation_diagnostics_with_db(db, resolved, options, pipeline)
    });
    // Persist the salsa snapshot after a successful CLI prepare so the next
    // invocation skips recomputation of unchanged persisted queries.
    if result.is_ok() {
        with_db(crate::persistence::persist_session_snapshot);
    }
    result
}

/// Run executable prepare with the process-scoped database (CLI / one-shot callers).
pub fn prepare_compilation(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    let result = with_db(|db| {
        ensure_db_for_resolved(db, resolved);
        prepare_compilation_with_db(db, resolved, options, pipeline)
    });
    // Persist the salsa snapshot after a successful CLI prepare so the next
    // invocation skips recomputation of unchanged persisted queries.
    if result.is_ok() {
        with_db(crate::persistence::persist_session_snapshot);
    }
    result
}

/// Build typed HIR from a resolved input using the shared DB + entry session registry (CLI / codegen).
pub fn compile_front_end_from_resolved_input(
    resolved: &ResolvedInput,
    options: FrontEndOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<FrontEndTypedResult> {
    let prepared =
        prepare_compilation(resolved, PrepareOptions { front_end: options, ..Default::default() }, pipeline)?;
    prepared.into_executable()
}
