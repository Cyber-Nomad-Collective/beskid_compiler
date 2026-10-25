//! Process-scoped `BeskidDatabase` for CLI and other callers without a long-lived session handle.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{
    ExecutableFrontEnd, FrontEndOptions, PrepareOptions, PreparedCompilation, ResolvedInput,
};
use beskid_pipeline::PipelineObserver;

use crate::db::{BeskidDatabase, reset_compilation_database};
use crate::entry::{prepare_compilation_diagnostics_with_db, prepare_compilation_with_db};

thread_local! {
    static COMPILATION_DB: RefCell<BeskidDatabase> = RefCell::new(BeskidDatabase::default());
    static CONFIGURED_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Access the shared compilation database for this thread.
pub fn with_db<T>(f: impl FnOnce(&mut BeskidDatabase) -> T) -> T {
    COMPILATION_DB.with(|db| f(&mut db.borrow_mut()))
}

/// Reset all process-scoped compilation state for the current thread.
///
/// Long-lived callers must use this between independent syntax assemblies so
/// source-unit registrations from one generation cannot leak into the next.
pub fn reset_process_compilation_database() {
    with_db(reset_compilation_database);
    CONFIGURED_ROOT.with(|configured| {
        *configured.borrow_mut() = None;
    });
}

/// Give each project a fresh in-memory database.
///
/// One-shot compiles neither load nor save the Salsa snapshot under `obj/beskid/cache/salsa/`.
/// Every compile registers its syntax again: restoring writes the rebuilt tree into each unit's
/// revision input, the source authorities are granted anew, and a Mod rewrite gives the assembly
/// a new generation. Each of those writes invalidates the memos that read the unit, which is
/// nearly all of them, so a restored snapshot is verified and recomputed in full and its load
/// and save only add time (measured on `corelib.bproj`: 10 s load and 5-13 s save for no reuse).
fn configure_db_in_place(db: &mut BeskidDatabase, project_root: &Path) {
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
    let already_configured = CONFIGURED_ROOT.with(|configured| configured.borrow().as_ref() == Some(&canonical));
    if already_configured {
        return;
    }
    reset_compilation_database(db);
    CONFIGURED_ROOT.with(|configured| {
        *configured.borrow_mut() = Some(canonical);
    });
}

/// Start a fresh in-memory database when the project root changes.
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
    with_db(|db| {
        ensure_db_for_resolved(db, resolved);
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
        ensure_db_for_resolved(db, resolved);
        prepare_compilation_with_db(db, resolved, options, pipeline)
    })
}

/// Build the typed front end of a Mod-free resolved input using the shared DB + entry session
/// registry (CLI / codegen). A plan with a native Mod dependency fails with an error that names
/// the Mod; use [`compile_front_end_with_mod_host`] for such plans.
pub fn compile_mod_free_front_end(
    resolved: &ResolvedInput,
    options: FrontEndOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ExecutableFrontEnd> {
    compile_front_end_with_mod_host(resolved, options, beskid_analysis::mod_host::ModHost::NoMods, pipeline)
}

/// Build the typed front end of a resolved input, running its native Mods in `mod_host`.
pub fn compile_front_end_with_mod_host(
    resolved: &ResolvedInput,
    options: FrontEndOptions,
    mod_host: beskid_analysis::mod_host::ModHost,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ExecutableFrontEnd> {
    let prepared = prepare_compilation(
        resolved,
        PrepareOptions { front_end: options, ..PrepareOptions::with_mod_host(mod_host) },
        pipeline,
    )?;
    prepared.into_executable()
}
