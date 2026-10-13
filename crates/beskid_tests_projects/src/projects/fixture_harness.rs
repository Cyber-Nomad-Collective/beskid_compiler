//! Shared fixture resolution and Salsa-backed assembly for integration tests.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use beskid_analysis::SemanticDiagnostic;
use beskid_analysis::mod_host::ModHost;
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, ResolvedInput, compile_plan_for_input_path, resolve_input,
    resolved_input_from_plan,
};
use beskid_tests_mod_host::ModHostScope;
use beskid_queries::{configure_db_for_project, program_assembly, with_db};

use super::core_env_lock::core_dependency_env_lock;
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

/// The shared native Mod host: the staged debug runtime kit as a value. It holds no lock and
/// never touches the process environment.
pub fn enter_mod_host() -> ModHostScope {
    ModHostScope::shared().expect("enter the shared native Mod host scope")
}

/// The qualified native Mod host for the compile plan of `resolved`.
pub fn mod_host_for(scope: &ModHostScope, resolved: &ResolvedInput) -> ModHost {
    let plan = resolved.compile_plan.as_ref().expect("Core-dependent input has a compile plan");
    scope.corelib_mod_host(plan).expect("qualify the native Mods of the compile plan")
}

/// Project-aware analysis of the file at `path` with the qualified native Mods of its compile plan,
/// through the same query-backed prepare spine `beskid check` runs. Native Mods read syntax
/// through the registered query authority, which only that spine installs.
pub fn analyze_file_with_mods(path: &Path) -> Result<Vec<SemanticDiagnostic>, String> {
    let source = std::fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let plan = compile_plan_for_input_path(path)
        .ok_or_else(|| format!("{} has no project compile plan", path.display()))?;
    let scope = enter_mod_host();
    let native_mods = scope.corelib_mod_host(&plan).map_err(|error| format!("{error:#}"))?;
    let resolved = resolved_input_from_plan(path.to_path_buf(), source, plan, None, None);
    let options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        ..PrepareOptions::with_mod_host(native_mods)
    };
    beskid_queries::prepare_compilation_diagnostics(&resolved, options, None)
        .map(|(_, diagnostics, _fixes)| diagnostics)
        .map_err(|error| format!("{error:#}"))
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures").join(name)
}

pub fn corelib_mvp_fixture() -> PathBuf {
    fixture_path("corelib_mvp")
}

pub fn try_expression_fixture() -> PathBuf {
    fixture_path("try_expression")
}

/// Compose cwd lock, optional std env lock, and Salsa persistence for a project fixture.
pub fn with_project_test_env<F: FnOnce()>(project_root: &Path, f: F) {
    let _env = core_dependency_env_lock();
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
        beskid_analysis::projects::WorkspacePrepareOptions::default(),
    )
    .expect("resolve fixture")
}

/// Populate `ResolvedInput.assembly` once via Salsa `program_assembly`.
pub fn resolve_fixture_with_assembly(fixture_root: &Path, entry: &str, target: &str) -> ResolvedInput {
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
static CORELIB_TESTS_ENTRY_ASSEMBLIES: Mutex<Option<HashMap<String, Arc<ProgramAssembly>>>> = Mutex::new(None);

pub fn corelib_tests_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib/tests/corelib_tests")
}

/// Resolve a `corelib_tests` entry through the analysis spine (no assembly yet).
pub fn resolve_corelib_tests_entry(entry_relative: &str) -> ResolvedInput {
    let root = corelib_tests_project_root();
    let entry_path = root.join("src").join(entry_relative);
    resolve_input(
        Some(&entry_path),
        Some(&root),
        None,
        None,
        beskid_analysis::projects::WorkspacePrepareOptions::default(),
    )
    .unwrap_or_else(|err| panic!("resolve corelib_tests entry {entry_relative}: {err}"))
}

/// Resolve and assemble a `corelib_tests` entry via Salsa [`program_assembly`].
pub fn resolve_corelib_tests_entry_with_assembly(entry_relative: &str) -> ResolvedInput {
    let mut resolved = resolve_corelib_tests_entry(entry_relative);
    let assembly = cached_corelib_tests_assembly(entry_relative, &resolved);
    resolved.assembly = Some((*assembly).clone());
    resolved
}

fn cached_corelib_tests_assembly(entry_relative: &str, resolved: &ResolvedInput) -> Arc<ProgramAssembly> {
    let mut guard = CORELIB_TESTS_ENTRY_ASSEMBLIES.lock().expect("corelib_tests assembly cache lock");
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

/// Lower a single test entrypoint from a `corelib_tests` file to CLIF (same path as `beskid test`).
pub fn lower_corelib_tests_entrypoint(entry_relative: &str, entrypoint: &str) -> beskid_codegen::CodegenArtifact {
    let resolved = resolve_corelib_tests_entry_with_assembly(entry_relative);
    let assembly = Arc::new(resolved.assembly.as_ref().expect("corelib entry assembly").clone());
    beskid_engine::services::lower_syntax_assembly_entrypoint(
        assembly,
        entrypoint,
        beskid_engine::host_runtime_target().unwrap_or_else(|error| panic!("host ABI-v5 target: {error}")),
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
            let resolved =
                with_project_test_env_return(&root, |root| resolve_fixture_with_assembly(root, "Src/Main.bd", "App"));
            Arc::new(resolved.assembly.expect("assembly"))
        })
        .clone()
}

/// Like [`with_project_test_env`], but returns a value. Caller must already hold the cwd lock
/// (e.g. via `with_project_test_env`); this helper must not re-enter `with_cwd_at_workspace_root`
/// or tests deadlock on `PROJECT_TEST_CWD_LOCK`.
fn with_project_test_env_return<T>(project_root: &Path, f: impl FnOnce(&Path) -> T) -> T {
    configure_db_for_project(project_root);
    f(project_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Entry resolution of the raw assembly through the module index (the resolver is a syntax
    /// authority, not a typing one).
    fn resolve_entry(assembly: &ProgramAssembly) -> beskid_analysis::resolve::Resolution {
        assembly
            .module_index
            .resolve_entry_program(&assembly.entry_unit().program, Some(&assembly.entry_unit().path), assembly)
            .expect("resolve the suite entry")
    }

    /// The production semantic gate of `beskid check`: the query-backed prepare spine with the
    /// qualified Corelib Mods reports no error for the suite entry. The job owns its database, so
    /// it never registers a generation in the process database other tests lower from.
    fn assert_production_semantic_gate(resolved: &ResolvedInput, suite: &str) {
        let scope = enter_mod_host();
        let options = PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..PrepareOptions::with_mod_host(mod_host_for(&scope, resolved))
        };
        let (_, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics_isolated(resolved, options, None)
            .unwrap_or_else(|error| panic!("{suite}: the production semantic gate must judge the suite: {error:#}"));
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == beskid_analysis::Severity::Error)
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{suite} must pass the production semantic gate: {errors:#?}");
    }

    /// Byte range of one former target's section in a merged `corelib_tests` suite entry.
    fn suite_section(source: &str, former_file: &str) -> std::ops::Range<usize> {
        let marker = format!("// ---- {former_file} ");
        let start = source.find(&marker).unwrap_or_else(|| panic!("suite has no section for {former_file}"));
        let end = source[start + marker.len()..].find("\n// ---- ").map_or(source.len(), |offset| start + marker.len() + offset);
        start..end
    }

    #[test]
    fn text_parser_tests_keep_parser_function_identity_and_signature_types() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("text/TextSuite.bd");
            let assembly = resolved.assembly.as_ref().expect("TextParser assembly");
            let started = Instant::now();
            let resolution = resolve_entry(assembly);
            let elapsed = started.elapsed();
            assert!(elapsed < Duration::from_secs(5), "TextParser declaration-only resolution took {elapsed:?}");
            assert!(
                resolution.tables.scoped_resolved_values.is_empty(),
                "dependency declaration resolution must not retain body value facts"
            );
            let dependency_locals = resolution
                .tables
                .locals
                .iter()
                .filter(|local| {
                    local
                        .source_path
                        .as_ref()
                        .is_some_and(|path| !beskid_analysis::paths::same_file(path, &assembly.entry_unit().path))
                })
                .collect::<Vec<_>>();
            assert!(
                dependency_locals.is_empty(),
                "dependency declaration resolution must not retain body locals: {dependency_locals:?}"
            );
            let section = suite_section(&resolved.source, "TextParserTests.bd");
            let parser_targets = resolution
                .tables
                .resolved_values
                .iter()
                .filter(|(span, _)| section.contains(&span.start))
                .filter_map(|(span, value)| {
                    let source = resolved.source.get(span.start..span.end)?;
                    let expected_name = ["Literal", "IsOk", "Choice2"]
                        .into_iter()
                        .find(|name| source.starts_with(&format!("Parser.{name}")))?;
                    let beskid_analysis::resolve::ResolvedValue::Item(item) = value else {
                        panic!("Parser.{expected_name} resolved to non-item {value:?}");
                    };
                    let target = beskid_analysis::resolve::qualified_name(&resolution, *item)
                        .expect("qualified Parser function target");
                    Some((expected_name, target))
                })
                .collect::<Vec<_>>();
            assert_eq!(parser_targets.iter().filter(|(name, _)| *name == "Literal").count(), 3);
            assert_eq!(parser_targets.iter().filter(|(name, _)| *name == "IsOk").count(), 4);
            assert_eq!(parser_targets.iter().filter(|(name, _)| *name == "Choice2").count(), 1);
            assert!(
                parser_targets.iter().all(|(name, target)| target.ends_with(&format!("::Core::Text::Parser::{name}"))),
                "Parser calls resolved to {parser_targets:?}"
            );
        });
    }

    #[test]
    fn collections_array_tests_keep_array_iterator_signature_types() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("collections/CollectionsSuite.bd");
            let assembly = resolved.assembly.as_ref().expect("ArrayTests assembly");
            let raw_resolution = resolve_entry(assembly);
            let array_unit = assembly
                .units
                .iter()
                .find(|unit| unit.path.ends_with("Core/Collections/Array.bd"))
                .expect("Array facade unit");
            let array_source = std::fs::read_to_string(&array_unit.path).expect("read Array facade");
            let array_types = raw_resolution
                .tables
                .scoped_resolved_types
                .iter()
                .find(|(path, _)| beskid_analysis::paths::same_file(path, &array_unit.path))
                .map(|(_, types)| types)
                .expect("Array facade type facts");
            let iterator_targets = array_types
                .iter()
                .filter_map(|(span, ty)| array_source.get(span.start..span.end).map(|source| (span, source, ty)))
                .filter(|(_, source, _)| source.contains("ArrayIter"))
                .filter_map(|(_, _, ty)| match ty {
                    beskid_analysis::resolve::ResolvedType::Item(item) => {
                        beskid_analysis::resolve::qualified_name(&raw_resolution, *item)
                    }
                    beskid_analysis::resolve::ResolvedType::Generic(_) => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(iterator_targets.len(), 6, "Array iterator declaration targets: {iterator_targets:?}");
            assert!(
                iterator_targets
                    .iter()
                    .all(|target| target.ends_with("::Core::Collections::Array::ArrayIter::ArrayIter")),
                "Array iterator declarations resolved to {iterator_targets:?}"
            );
            assert!(
                raw_resolution.tables.scoped_resolved_values.is_empty(),
                "dependency declaration resolution must not retain body value facts"
            );
            assert_production_semantic_gate(&resolved, "ArrayTests");
        });
    }

    #[test]
    fn fs_tests_nested_enum_pattern_bindings_pass_the_production_semantic_gate() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("system/SystemSuite.bd");
            assert_production_semantic_gate(&resolved, "FS");
        });
    }

    #[test]
    fn system_environment_error_constructors_lower_from_their_dependency_unit() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            lower_corelib_tests_entrypoint(
                "system/SystemSuite.bd",
                "env_get_empty_name_returns_invalid_name_error",
            );
            lower_corelib_tests_entrypoint(
                "system/SystemSuite.bd",
                "env_set_empty_name_returns_invalid_name_error",
            );
        });
    }

    #[test]
    fn query_tests_pass_the_production_semantic_gate() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("query/QuerySuite.bd");
            let assembly = resolved.assembly.as_ref().expect("query assembly");
            let resolution = resolve_entry(assembly);
            let section = suite_section(&resolved.source, "QueryTests.bd");
            let current_targets = resolution
                .tables
                .resolved_values
                .iter()
                .filter(|(span, _)| section.contains(&span.start))
                .filter(|(span, _)| {
                    resolved
                        .source
                        .get(span.start..span.end)
                        .is_some_and(|source| source.starts_with("ArrayIterator.Current"))
                })
                .map(|(_, value)| match value {
                    beskid_analysis::resolve::ResolvedValue::Item(item) => {
                        beskid_analysis::resolve::qualified_name(&resolution, *item).expect("qualified Current target")
                    }
                    other => panic!("ArrayIterator.Current resolved to non-item {other:?}"),
                })
                .collect::<Vec<_>>();
            assert_eq!(current_targets.len(), 3);
            assert!(
                current_targets.iter().all(|target| target.ends_with("::Query::ArrayIterator::Current")),
                "ArrayIterator.Current calls resolved to {current_targets:?}"
            );
            assert_production_semantic_gate(&resolved, "QueryTests");
        });
    }

    #[test]
    fn corelib_entry_assemblies_remain_isolated_by_explicit_source_path() {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let channel = resolve_corelib_tests_entry_with_assembly("concurrency/ConcurrencySuite.bd");
            let messages = resolve_corelib_tests_entry_with_assembly("console/ConsoleSuite.bd");

            assert!(channel.source_path.ends_with("concurrency/ConcurrencySuite.bd"));
            assert!(messages.source_path.ends_with("console/ConsoleSuite.bd"));
            assert!(
                channel
                    .assembly
                    .as_ref()
                    .expect("channel assembly")
                    .entry_unit()
                    .path
                    .ends_with("concurrency/ConcurrencySuite.bd")
            );
            assert!(
                messages
                    .assembly
                    .as_ref()
                    .expect("messages assembly")
                    .entry_unit()
                    .path
                    .ends_with("console/ConsoleSuite.bd")
            );
        });
    }
}
