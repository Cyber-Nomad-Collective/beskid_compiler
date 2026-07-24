use std::path::PathBuf;

use crate::projects::fixture_harness::{corelib_mvp_fixture, shared_corelib_mvp_assembly, with_project_test_env};
use crate::projects::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};
use beskid_analysis::projects::{
    AssemblyDiscovery, AssemblyOptions, ProgramAssembly, effective_roots_for_plan, module_roots_from_effective,
};
use beskid_analysis::services::resolve_input;
use beskid_pipeline::phases::{FULL_BUILD_PHASE_ORDER, PROGRAM_ASSEMBLE, WORKSPACE_MATERIALIZE};
use beskid_queries::{configure_db_for_project, program_assembly, with_db};

#[test]
fn full_build_phase_order_includes_program_assemble() {
    let order = FULL_BUILD_PHASE_ORDER;
    let mat = order.iter().position(|p| *p == WORKSPACE_MATERIALIZE).unwrap();
    let asm = order.iter().position(|p| *p == PROGRAM_ASSEMBLE).unwrap();
    let parse = order.iter().position(|p| *p == "parse").unwrap();
    assert!(mat < asm);
    assert!(asm < parse);
}

#[test]
fn effective_roots_prefers_materialized_corelib_mvp_fixture() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(Some(&fixture.join("Src/Main.bd")), Some(&fixture), None, None, false, false)
            .expect("resolve corelib_mvp");

        let plan = resolved.compile_plan.expect("compile plan");
        let roots = effective_roots_for_plan(&plan, resolved.prepared_workspace.as_ref());
        let module_roots = module_roots_from_effective(&roots);

        assert!(
            module_roots.iter().any(|root| root.display().to_string().contains("obj/beskid/deps")),
            "expected materialized dependency root in {module_roots:?}"
        );
    });
}

#[test]
fn assembly_closure_loads_std_units_for_corelib_mvp() {
    with_project_test_env(&corelib_mvp_fixture(), || {
        let assembly = shared_corelib_mvp_assembly();
        let loaded: Vec<String> = assembly.units.iter().map(|unit| unit.path.display().to_string()).collect();

        assert!(
            loaded.iter().any(|p| p.contains("Core") && p.contains("Output")),
            "expected Core.Output module in assembly closure, got: {loaded:?}"
        );
    });
}

#[test]
fn workspace_scan_respects_max_units() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(Some(&fixture.join("Src/Main.bd")), Some(&fixture), None, None, false, false)
            .expect("resolve");
        let plan = resolved.compile_plan.expect("plan");
        let options =
            AssemblyOptions { discovery: AssemblyDiscovery::WorkspaceScan, max_units: 2, ..Default::default() };

        configure_db_for_project(&fixture);
        let err = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &resolved.source_path,
                Some(&resolved.source),
                &options,
            )
        })
        .expect_err("should hit max_units");
        assert!(err.to_string().contains("max_units"));
    });
}

#[test]
fn module_index_known_paths_include_std_io_for_corelib_mvp() {
    with_project_test_env(&corelib_mvp_fixture(), || {
        let assembly = shared_corelib_mvp_assembly();
        let paths = assembly.module_index.known_module_path_strings();
        assert!(
            paths.contains("Std::Core::Output"),
            "expected Std::Core::Output in known module paths, got: {paths:?}"
        );
    });
}

#[test]
fn module_index_resolve_entry_succeeds_for_corelib_mvp_main() {
    with_project_test_env(&corelib_mvp_fixture(), || {
        let assembly = shared_corelib_mvp_assembly();
        let resolution = assembly
            .module_index
            .resolve_entry(&assembly.entry_unit().program)
            .expect("entry resolve with use aliases");

        let print_item = resolution
            .items
            .iter()
            .find(|item| item.name == "WriteLine" && item.kind == beskid_analysis::resolve::ItemKind::Function);
        assert!(print_item.is_some(), "expected WriteLine in resolution items");

        let use_io = resolution
            .items
            .iter()
            .find(|item| item.kind == beskid_analysis::resolve::ItemKind::Use && item.name == "Output");
        assert!(use_io.is_none(), "use aliases should not remain as callable scope items");
    });
}

#[test]
fn corelib_syscall_tests_prefetch_includes_testing_assert_true() {
    use crate::projects::corelib::corelib_root;

    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let project = corelib_root().join("tests/corelib_tests");
        let entry = project.join("src/console/AnsiEscapeTests.bd");
        let resolved = resolve_input(Some(&entry), Some(&project), Some("ConsoleAnsiEscapeTests"), None, false, false)
            .expect("resolve corelib_tests console target");

        let plan = resolved.compile_plan.expect("compile plan");
        let options = AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..Default::default() };
        configure_db_for_project(&project);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &resolved.source_path,
                Some(&resolved.source),
                &options,
            )
        })
        .expect("assemble console tests");

        let loaded: Vec<String> = assembly.units.iter().map(|unit| unit.path.display().to_string()).collect();
        assert!(
            loaded.iter().any(|p| p.contains("Testing/Assert.bd")),
            "expected Testing.Assert in import closure, got: {loaded:?}"
        );
        assert!(
            loaded.iter().any(|p| p.ends_with("AnsiEscapeTests.bd")),
            "expected entry unit in assembly, got: {loaded:?}"
        );
    });
}

#[test]
fn parallel_unit_build_matches_serial_assembly_order() {
    use crate::projects::std_dependency_env_lock;

    let _guard = std_dependency_env_lock();
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(Some(&fixture.join("Src/Main.bd")), Some(&fixture), None, None, false, false)
            .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        let options = AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..Default::default() };

        configure_db_for_project(&fixture);
        let serial = assemble_with_thread_cap(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            &resolved.source,
            &options,
            1,
        );
        let parallel = assemble_with_thread_cap(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            &resolved.source,
            &options,
            4,
        );

        let serial_paths: Vec<_> = serial.units.iter().map(|unit| unit.path.display().to_string()).collect();
        let parallel_paths: Vec<_> = parallel.units.iter().map(|unit| unit.path.display().to_string()).collect();

        assert_eq!(serial_paths, parallel_paths, "parallel assembly must preserve deterministic unit order");
        assert_eq!(serial.entry_index, parallel.entry_index);
        assert_eq!(
            serial.units.len(),
            parallel.units.len(),
            "unit count must match between serial and parallel builds"
        );
    });
}

fn assemble_with_thread_cap(
    plan: &beskid_analysis::projects::CompilePlan,
    workspace: Option<&beskid_analysis::projects::PreparedProjectWorkspace>,
    entry_path: &std::path::Path,
    entry_source: &str,
    options: &AssemblyOptions,
    threads: usize,
) -> ProgramAssembly {
    // SAFETY: test runs single-threaded under `std_dependency_env_lock`.
    unsafe {
        std::env::set_var("BESKID_ASSEMBLY_THREADS", threads.to_string());
    }
    with_db(|db| program_assembly(db, plan, workspace, entry_path, Some(entry_source), options))
        .expect("assemble with thread cap")
}
