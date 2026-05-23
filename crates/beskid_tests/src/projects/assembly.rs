use std::path::PathBuf;

use crate::projects::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};
use beskid_analysis::projects::{
    AssemblyDiscovery, AssemblyOptions, assemble_program, effective_roots_for_plan,
    module_roots_from_effective,
};
use beskid_analysis::services::resolve_input;
use beskid_pipeline::phases::{FULL_BUILD_PHASE_ORDER, PROGRAM_ASSEMBLE, WORKSPACE_MATERIALIZE};

#[test]
fn full_build_phase_order_includes_program_assemble() {
    let order = FULL_BUILD_PHASE_ORDER;
    let mat = order
        .iter()
        .position(|p| *p == WORKSPACE_MATERIALIZE)
        .unwrap();
    let asm = order.iter().position(|p| *p == PROGRAM_ASSEMBLE).unwrap();
    let parse = order.iter().position(|p| *p == "parse").unwrap();
    assert!(mat < asm);
    assert!(asm < parse);
}

#[test]
fn effective_roots_prefers_materialized_corelib_mvp_fixture() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            None,
            None,
            false,
            false,
        )
        .expect("resolve corelib_mvp");

        let plan = resolved.compile_plan.expect("compile plan");
        let roots = effective_roots_for_plan(&plan, resolved.prepared_workspace.as_ref());
        let module_roots = module_roots_from_effective(&roots);

        assert!(
            module_roots
                .iter()
                .any(|root| root.display().to_string().contains("obj/beskid/deps")),
            "expected materialized dependency root in {module_roots:?}"
        );
    });
}

#[test]
fn assembly_closure_loads_std_units_for_corelib_mvp() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            None,
            None,
            false,
            false,
        )
        .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::ImportClosure,
            ..Default::default()
        };

        let assembly = assemble_program(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &options,
        )
        .expect("assemble corelib_mvp");

        let loaded: Vec<String> = assembly
            .units
            .iter()
            .map(|unit| unit.path.display().to_string())
            .collect();

        assert!(
            loaded
                .iter()
                .any(|p| p.contains("System") && p.contains("Output")),
            "expected System.Output module in assembly closure, got: {loaded:?}"
        );
    });
}

#[test]
fn workspace_scan_respects_max_units() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            None,
            None,
            false,
            false,
        )
        .expect("resolve");
        let plan = resolved.compile_plan.expect("plan");
        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::WorkspaceScan,
            max_units: 2,
            ..Default::default()
        };

        let err = assemble_program(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &options,
        )
        .expect_err("should hit max_units");
        assert!(err.to_string().contains("max_units"));
    });
}

#[test]
fn module_index_known_paths_include_std_io_for_corelib_mvp() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            None,
            None,
            false,
            false,
        )
        .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::ImportClosure,
            ..Default::default()
        };

        let assembly = assemble_program(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &options,
        )
        .expect("assemble");

        let paths = assembly.module_index.known_module_path_strings();
        assert!(
            paths.contains("Std::System::Output"),
            "expected Std::System::Output in known module paths, got: {paths:?}"
        );
    });
}

#[test]
fn module_index_resolve_entry_succeeds_for_corelib_mvp_main() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            None,
            None,
            false,
            false,
        )
        .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::ImportClosure,
            ..Default::default()
        };

        let assembly = assemble_program(
            &plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &options,
        )
        .expect("assemble");

        let resolution = assembly
            .module_index
            .resolve_entry(&assembly.entry_unit().program)
            .expect("entry resolve with use aliases");

        let print_item = resolution.items.iter().find(|item| {
            item.name == "WriteLine" && item.kind == beskid_analysis::resolve::ItemKind::Function
        });
        assert!(
            print_item.is_some(),
            "expected WriteLine in resolution items"
        );

        let use_io = resolution
            .items
            .iter()
            .find(|item| item.kind == beskid_analysis::resolve::ItemKind::Use && item.name == "Output");
        assert!(
            use_io.is_none(),
            "use aliases should not remain as callable scope items"
        );
    });
}
