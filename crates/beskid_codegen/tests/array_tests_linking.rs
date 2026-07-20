//! Lower `CollectionsArrayTests` without JIT to isolate link-plan / codegen stack issues.
#![cfg(feature = "slow")]

use std::path::PathBuf;

use beskid_analysis::services::{FrontEndOptions, ResolvedInput, resolve_input};
use beskid_codegen::linking::{FunctionDefIndex, LinkPlan};
use beskid_codegen::lowering::lower_program_with_assembly;
use beskid_queries::compile_front_end_from_resolved_input;

fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("compiler workspace root")
        .to_path_buf()
}

#[test]
fn assemble_array_tests_workspace() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/collections/ArrayTests.bd");
    let project_root: PathBuf = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read ArrayTests.bd");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve");
    std::env::set_current_dir(previous).expect("restore cwd");
    let plan = resolved.compile_plan.expect("compile plan");
    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };
    let front = compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end");
    assert!(front.assembly.hir_units.len() > 5);
}

#[test]
fn front_end_array_tests() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/collections/ArrayTests.bd");
    let project_root: PathBuf = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read ArrayTests.bd");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve");
    std::env::set_current_dir(previous).expect("restore cwd");
    let plan = resolved.compile_plan.expect("compile plan");
    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };
    let _front = compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end");
}

#[test]
fn lower_collections_array_tests_artifact_without_jit() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/collections/ArrayTests.bd");
    let project_root: PathBuf = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read ArrayTests.bd");

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir workspace");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve");
    std::env::set_current_dir(previous).expect("restore cwd");

    let plan = resolved.compile_plan.expect("compile plan");

    let resolved_input = ResolvedInput {
        source_path: entry.clone(),
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };

    let front = compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end");
    assert!(
        front.assembly.hir_units.len() > 1,
        "expected dependency units"
    );

    let assembly = &front.assembly;
    let def_index = FunctionDefIndex::build(&front.resolution, &assembly.hir_units);
    let plan = LinkPlan::build(&front.hir, &front.resolution, &front.typed, &def_index);
    assert!(plan.callees.len() < 32, "callees={}", plan.callees.len());
    assert!(
        def_index.functions().len() < 512,
        "indexed functions={}",
        def_index.functions().len()
    );
}

#[test]
fn lower_collections_array_tests_artifact_validates() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/collections/ArrayTests.bd");
    let project_root: PathBuf = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read ArrayTests.bd");

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir workspace");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve");
    std::env::set_current_dir(previous).expect("restore cwd");

    let plan = resolved.compile_plan.expect("compile plan");

    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };

    let front = compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end");
    let def_index = FunctionDefIndex::build(&front.resolution, &front.assembly.hir_units);
    let plan = LinkPlan::build(&front.hir, &front.resolution, &front.typed, &def_index);
    assert!(
        plan.emitted_symbol_names(&front.resolution)
            .iter()
            .any(|name| name.contains("Equal")),
        "link plan should reach Testing.Assert.Equal"
    );

    match lower_program_with_assembly(
        &front.hir,
        &front.resolution,
        &front.typed,
        Some(&front.assembly),
    ) {
        Ok(_) => panic!("retired HIR driver must reject without fallback"),
        Err(errors) => {
            let message = errors
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            assert!(
                message.contains(beskid_codegen::RETIRED_HIR_LOWERING_PATH),
                "{message}"
            );
        }
    }
}
