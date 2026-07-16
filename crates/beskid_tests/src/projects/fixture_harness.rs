//! Shared fixture resolution and Salsa-backed assembly for integration tests.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{FrontEndOptions, PrepareOptions, ResolvedInput, resolve_input};
use beskid_queries::{
    compile_front_end_from_resolved_input, configure_db_for_project, prepare_compilation_with_db,
    program_assembly, with_db,
};

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
pub fn resolve_fixture(fixture_root: &Path, entry: &str, target: &str) -> ResolvedInput {
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
static CORELIB_TESTS_ENTRY_ASSEMBLIES: Mutex<Option<HashMap<String, Arc<ProgramAssembly>>>> =
    Mutex::new(None);

pub fn corelib_tests_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/beskid_corelib/tests/corelib_tests")
}

/// Resolve a `corelib_tests` entry through the analysis spine (no assembly yet).
pub fn resolve_corelib_tests_entry(entry_relative: &str) -> ResolvedInput {
    let root = corelib_tests_project_root();
    let entry_path = root.join("src").join(entry_relative);
    resolve_input(Some(&entry_path), Some(&root), None, None, false, false)
        .unwrap_or_else(|err| panic!("resolve corelib_tests entry {entry_relative}: {err}"))
}

/// Resolve and assemble a `corelib_tests` entry via Salsa [`program_assembly`].
pub fn resolve_corelib_tests_entry_with_assembly(entry_relative: &str) -> ResolvedInput {
    let mut resolved = resolve_corelib_tests_entry(entry_relative);
    let assembly = cached_corelib_tests_assembly(entry_relative, &resolved);
    resolved.assembly = Some((*assembly).clone());
    resolved
}

fn cached_corelib_tests_assembly(
    entry_relative: &str,
    resolved: &ResolvedInput,
) -> Arc<ProgramAssembly> {
    let mut guard = CORELIB_TESTS_ENTRY_ASSEMBLIES
        .lock()
        .expect("corelib_tests assembly cache lock");
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    let cache = guard.as_mut().expect("corelib_tests assembly cache");
    if let Some(assembly) = cache.get(entry_relative) {
        test_progress(&format!("↺ corelib assembly cache hit: {entry_relative}"));
        return Arc::clone(assembly);
    }

    test_progress(&format!("⋯ corelib program assembly: {entry_relative}"));
    let assemble_started = Instant::now();
    let plan = resolved.compile_plan.clone().expect("compile plan");
    let assembly = Arc::new(
        with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &resolved.source_path,
                Some(&resolved.source),
                &beskid_analysis::projects::assembly_options_for_plan(&plan),
            )
        })
        .unwrap_or_else(|err| {
            panic!("program_assembly for {entry_relative}: {err}");
        }),
    );
    test_progress(&format!(
        "⋯ corelib program assembly done: {entry_relative} ({:.1}s)",
        assemble_started.elapsed().as_secs_f64()
    ));
    cache.insert(entry_relative.to_owned(), Arc::clone(&assembly));
    assembly
}

/// Semantic gate for a `corelib_tests` entry (resolve + typecheck entry body; dependency signatures only).
pub fn typecheck_corelib_tests_entry(entry_relative: &str) {
    test_progress(&format!("→ corelib typecheck: {entry_relative}"));
    let started = Instant::now();
    let resolved = resolve_corelib_tests_entry_with_assembly(entry_relative);
    with_db(|db| {
        prepare_compilation_with_db(
            db,
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            None,
        )
    })
    .expect("corelib_tests semantic gate");
    test_progress(&format!(
        "✓ corelib typecheck: {entry_relative} ({:.1}s)",
        started.elapsed().as_secs_f64()
    ));
}

/// Lower a single test entrypoint from a `corelib_tests` file to CLIF (same path as `beskid test`).
pub fn lower_corelib_tests_entrypoint(
    entry_relative: &str,
    entrypoint: &str,
) -> beskid_codegen::CodegenArtifact {
    let resolved = resolve_corelib_tests_entry_with_assembly(entry_relative);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .unwrap_or_else(|err| panic!("front-end for {entry_relative}: {err}"));
    beskid_engine::services::lower_prepared_syntax_entrypoint(
        &front,
        entrypoint,
        beskid_engine::host_runtime_target()
            .unwrap_or_else(|error| panic!("host ABI-v5 target: {error}")),
    )
    .unwrap_or_else(|err| panic!("lower {entrypoint} in {entry_relative}: {err}"))
}

fn test_progress(message: &str) {
    if std::env::var("BESKID_TEST_QUIET").is_ok() {
        return;
    }
    let mut err = std::io::stderr();
    let _ = writeln!(err, "{message}");
    let _ = err.flush();
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corelib_entry_assemblies_remain_isolated_by_explicit_source_path() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let channel = resolve_corelib_tests_entry_with_assembly(
                "concurrency/ChannelApiTests.bd",
            );
            let messages = resolve_corelib_tests_entry_with_assembly(
                "console/ConsoleMessageChannelTests.bd",
            );

            assert!(channel
                .source_path
                .ends_with("concurrency/ChannelApiTests.bd"));
            assert!(messages
                .source_path
                .ends_with("console/ConsoleMessageChannelTests.bd"));
            assert!(channel
                .assembly
                .as_ref()
                .expect("channel assembly")
                .entry_unit()
                .path
                .ends_with("concurrency/ChannelApiTests.bd"));
            assert!(messages
                .assembly
                .as_ref()
                .expect("messages assembly")
                .entry_unit()
                .path
                .ends_with("console/ConsoleMessageChannelTests.bd"));
        });
    }
}

/// Like [`with_project_test_env`], but returns a value. Caller must already hold the cwd lock
/// (e.g. via `with_project_test_env`); this helper must not re-enter `with_cwd_at_workspace_root`
/// or tests deadlock on `PROJECT_TEST_CWD_LOCK`.
fn with_project_test_env_return<T>(project_root: &Path, f: impl FnOnce(&Path) -> T) -> T {
    configure_db_for_project(project_root);
    f(project_root)
}
