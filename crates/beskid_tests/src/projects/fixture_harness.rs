//! Shared fixture resolution and Salsa-backed assembly for integration tests.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{PrepareMode, PrepareOptions, ResolvedInput, resolve_input};
use beskid_queries::{configure_db_for_project, prepare_compilation_with_db, program_assembly, with_db};

use super::std_env_lock::std_dependency_env_lock;
use super::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};

/// Linux CI runners use a smaller default thread stack than macOS; corelib lowering needs more headroom.
pub fn with_large_test_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("spawn large-stack test thread")
        .join()
        .expect("join large-stack test thread");
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_e2e_tests/fixtures")
        .join(name)
}

pub fn corelib_mvp_fixture() -> PathBuf {
    fixture_path("corelib_mvp")
}

pub fn try_expression_fixture() -> PathBuf {
    fixture_path("try_expression")
}

/// Compose cwd lock, optional std env lock, and Salsa persistence for a project fixture.
pub fn with_project_test_env<F: FnOnce()>(project_root: &Path, f: F) {
    let _env = std_dependency_env_lock();
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        configure_db_for_project(project_root);
        f();
    });
}

/// Resolve a fixture through the analysis spine (no assembly yet).
pub fn resolve_fixture(
    fixture_root: &Path,
    entry: &str,
    target: &str,
) -> ResolvedInput {
    let root = fixture_root.to_path_buf();
    let entry_path = root.join(entry);
    resolve_input(
        Some(&entry_path),
        Some(&root),
        Some(target),
        None,
        false,
        false,
    )
    .expect("resolve fixture")
}

/// Populate `ResolvedInput.assembly` once via Salsa `program_assembly`.
pub fn resolve_fixture_with_assembly(
    fixture_root: &Path,
    entry: &str,
    target: &str,
) -> ResolvedInput {
    let mut resolved = resolve_fixture(fixture_root, entry, target);
    let plan = resolved.compile_plan.clone().expect("compile plan");
    let assembly = with_db(|db| {
        program_assembly(
            db,
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &Default::default(),
        )
    })
    .expect("program_assembly");
    resolved.assembly = Some(assembly);
    resolved
}

static CORELIB_MVP_ASSEMBLY: OnceLock<Arc<ProgramAssembly>> = OnceLock::new();

pub fn shared_corelib_mvp_assembly() -> Arc<ProgramAssembly> {
    CORELIB_MVP_ASSEMBLY
        .get_or_init(|| {
            let root = corelib_mvp_fixture();
            let resolved = with_project_test_env_return(&root, |root| {
                resolve_fixture_with_assembly(root, "Src/Main.bd", "App")
            });
            Arc::new(resolved.assembly.expect("assembly"))
        })
        .clone()
}

/// Like [`with_project_test_env`], but returns a value. Caller must already hold the cwd lock
/// (e.g. via `with_project_test_env`); this helper must not re-enter `with_cwd_at_workspace_root`
/// or tests deadlock on `PROJECT_TEST_CWD_LOCK`.
fn with_project_test_env_return<T>(
    project_root: &Path,
    f: impl FnOnce(&Path) -> T,
) -> T {
    configure_db_for_project(project_root);
    f(project_root)
}

pub fn prepare_executable(resolved: &ResolvedInput) -> beskid_analysis::services::PreparedCompilation {
    with_db(|db| {
        prepare_compilation_with_db(
            db,
            resolved,
            PrepareOptions {
                mode: PrepareMode::Executable,
                ..Default::default()
            },
            None,
        )
    })
    .expect("prepare executable")
}
