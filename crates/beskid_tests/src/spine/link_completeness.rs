//! Link-plan lowering must pass `validate_artifact` for project entrypoints.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::projects::{AssemblyDiscovery, AssemblyOptions};
use beskid_analysis::services::{
    compile_front_end_from_resolved_input, resolve_input, resolved_input_from_plan,
    FrontEndOptions, ResolvedInput,
};
use beskid_queries::{configure_db_for_project, program_assembly, with_db};
use beskid_analysis::CompilationContext;
use beskid_codegen::linking::{FunctionDefIndex, LinkPlan, LinkSymbol};
use beskid_codegen::lowering::{
    lower_program_with_assembly, lower_program_with_assembly_for_entrypoint,
};
use beskid_codegen::validate_artifact;

use crate::projects::{compiler_workspace_root, with_cwd, with_cwd_at_workspace_root};
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

#[test]
fn main_entry_link_plan_validates_for_temp_project() {
    let root = temp_case_dir("spine_link_completeness");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "LinkSmoke"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let source = r#"
i32 helper() {
    return 7;
}

i32 main() {
    return helper();
}
"#;
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd(&root, || {
        let ctx = CompilationContext::try_for_analysis_path(&entry, None).expect("context");
        let plan = ctx.compile_plan.clone().expect("plan");
        let resolved = resolved_input_from_plan(
            entry.clone(),
            source.to_string(),
            plan.clone(),
            ctx.prepared_workspace.clone(),
            None,
        );
        configure_db_for_project(&root);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &entry,
                Some(source),
                &AssemblyOptions {
                    discovery: AssemblyDiscovery::ImportClosure,
                    include_std_prelude: false,
                    ..Default::default()
                },
            )
        })
        .expect("assemble");

        let front =
            compile_front_end_from_resolved_input(&resolved, FrontEndOptions::default(), None)
                .expect("front end");

        let def_index = FunctionDefIndex::build(&front.resolution, &assembly.hir_units);
        let link_plan = LinkPlan::build_for_entrypoint(
            &front.hir,
            "main",
            Some(&resolved.source_path),
            &front.resolution,
            &front.typed,
            &def_index,
        );
        assert!(
            !link_plan.entries.is_empty(),
            "temp project should expose a main entry"
        );

        let artifact = lower_program_with_assembly_for_entrypoint(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&assembly),
            Some("main"),
        )
        .expect("lower");

        validate_artifact(&artifact).expect("link plan symbols must be present in artifact");
    });

    let _ = fs::remove_dir_all(root);
}

/// Corelib `Testing.Assertions.AssertEqualI64` must survive dependency link plans and validate.
#[cfg(feature = "slow")]
#[test]
fn corelib_assert_equal_i64_link_plan_validates() {
    let root = compiler_workspace_root();
    with_cwd_at_workspace_root(&root, || {
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
        let source = fs::read_to_string(&entry).expect("read ArrayTests.bd");

        let resolved = resolve_input(
            Some(&entry),
            Some(&project_root),
            None,
            None,
            false,
            false,
        )
        .expect("resolve");

        let plan = resolved.compile_plan.expect("compile plan");
        configure_db_for_project(&project_root);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &entry,
                Some(&source),
                &AssemblyOptions {
                    discovery: AssemblyDiscovery::ImportClosure,
                    ..Default::default()
                },
            )
        })
        .expect("assemble");

        let resolved_input = ResolvedInput {
            source_path: entry,
            source,
            compile_plan: Some(plan),
            prepared_workspace: resolved.prepared_workspace,
            workspace_summary: resolved.workspace_summary,
            assembly: Some(assembly),
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
        let link_plan = LinkPlan::build(&front.hir, &front.resolution, &front.typed, &def_index);
        assert!(
            link_plan
                .emitted_symbol_names(&front.resolution)
                .iter()
                .any(|name| name.contains("AssertEqualI64")),
            "link plan should reach Testing.Assertions.AssertEqualI64"
        );

        let artifact = lower_program_with_assembly(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&front.assembly),
        )
        .expect("lower corelib array tests");

        let names: Vec<_> = artifact.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            names.iter().any(|name| name.contains("AssertEqualI64")),
            "expected AssertEqualI64 in artifact, have {} symbols",
            names.len()
        );
        validate_artifact(&artifact).expect("AssertEqualI64 link plan must validate");
    });
}

/// `ansi_cursor_builder_home` must reach the Capabilities/Terminal symbol chain via link plan.
#[cfg(feature = "slow")]
#[test]
fn link_plan_includes_capabilities_terminal_chain_for_ansi_cursor_builder_home() {
    let root = compiler_workspace_root();
    with_cwd_at_workspace_root(&root, || {
        let entry = root.join(
            "corelib/beskid_corelib/tests/corelib_tests/src/console/AnsiEscapeTests.bd",
        );
        let project_root: PathBuf = entry
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let source = fs::read_to_string(&entry).expect("read AnsiEscapeTests.bd");

        let resolved = resolve_input(
            Some(&entry),
            Some(&project_root),
            None,
            None,
            false,
            false,
        )
        .expect("resolve");

        let plan = resolved.compile_plan.expect("compile plan");
        configure_db_for_project(&project_root);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &entry,
                Some(&source),
                &AssemblyOptions {
                    discovery: AssemblyDiscovery::ImportClosure,
                    ..Default::default()
                },
            )
        })
        .expect("assemble");

        let resolved_input = ResolvedInput {
            source_path: entry.clone(),
            source,
            compile_plan: Some(plan),
            prepared_workspace: resolved.prepared_workspace,
            workspace_summary: resolved.workspace_summary,
            assembly: Some(assembly),
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
        let link_plan = LinkPlan::build_for_entrypoint(
            &front.hir,
            "ansi_cursor_builder_home",
            Some(&front.assembly.entry_unit().path),
            &front.resolution,
            &front.typed,
            &def_index,
        );

        let reachable: Vec<String> = link_plan
            .callees
            .iter()
            .chain(link_plan.entries.iter())
            .filter_map(|symbol| match symbol {
                LinkSymbol::Function { item, .. } | LinkSymbol::Method { item, .. } => {
                    beskid_analysis::resolve::qualified_name(&front.resolution, *item)
                }
                LinkSymbol::Test { item, .. } => {
                    beskid_analysis::resolve::qualified_name(&front.resolution, *item)
                }
            })
            .collect();

        for needle in [
            "ShouldEmitAnsi",
            "ProbeStdout",
            "IntoSequence",
            "WhenEnabled",
            "IsAtty",
            "Esc",
        ] {
            assert!(
                reachable.iter().any(|name| name.contains(needle)),
                "link plan should reach `{needle}`, have: {reachable:?}"
            );
        }

        for needle in ["ShouldEmitAnsi", "ProbeStdout", "EnvFlagSet"] {
            let items: Vec<_> = front
                .resolution
                .items
                .iter()
                .filter(|info| info.name.contains(needle) && info.kind == beskid_analysis::resolve::ItemKind::Function)
                .collect();
            assert!(
                !items.is_empty(),
                "expected function item for {needle}, have none"
            );
            for info in &items {
                assert!(
                    front.typed.function_signatures.contains_key(&info.id),
                    "missing signature for {} id {:?} span {:?} source {:?}",
                    info.name,
                    info.id,
                    info.span,
                    info.source_path
                );
                assert!(
                    def_index.function(info.id).is_some(),
                    "def_index missing {} id {:?} span {:?} source {:?}",
                    info.name,
                    info.id,
                    info.span,
                    info.source_path
                );
            }
        }

        for symbol in link_plan.callees.iter().chain(link_plan.entries.iter()) {
            let item = match symbol {
                LinkSymbol::Function { item, .. }
                | LinkSymbol::Method { item, .. }
                | LinkSymbol::Test { item, .. } => *item,
            };
            assert!(
                front.typed.function_signatures.contains_key(&item),
                "link plan item {:?} ({:?}) missing function signature",
                item,
                beskid_analysis::resolve::qualified_name(&front.resolution, item)
            );
        }

        let artifact = lower_program_with_assembly_for_entrypoint(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&front.assembly),
            Some("ansi_cursor_builder_home"),
        )
        .expect("lower ansi_cursor_builder_home");
        validate_artifact(&artifact).expect("ansi_cursor_builder_home link plan must validate");
    });
}

#[cfg(feature = "slow")]
#[test]
fn ansi_csi_bold_red_link_plan_validates() {
    let root = compiler_workspace_root();
    with_cwd_at_workspace_root(&root, || {
        let entry = root.join(
            "corelib/beskid_corelib/tests/corelib_tests/src/console/AnsiEscapeTests.bd",
        );
        let project_root: PathBuf = entry
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let source = fs::read_to_string(&entry).expect("read AnsiEscapeTests.bd");

        let resolved = resolve_input(
            Some(&entry),
            Some(&project_root),
            None,
            None,
            false,
            false,
        )
        .expect("resolve");

        let plan = resolved.compile_plan.expect("compile plan");
        configure_db_for_project(&project_root);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &entry,
                Some(&source),
                &AssemblyOptions {
                    discovery: AssemblyDiscovery::ImportClosure,
                    ..Default::default()
                },
            )
        })
        .expect("assemble");

        let resolved_input = ResolvedInput {
            source_path: entry.clone(),
            source: source.clone(),
            compile_plan: Some(plan),
            prepared_workspace: resolved.prepared_workspace,
            workspace_summary: resolved.workspace_summary,
            assembly: Some(assembly.clone()),
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

        let def_index = FunctionDefIndex::build(&front.resolution, &assembly.hir_units);
        let link_plan = LinkPlan::build_for_entrypoint(
            &front.hir,
            "ansi_csi_bold_red",
            Some(&assembly.entry_unit().path),
            &front.resolution,
            &front.typed,
            &def_index,
        );
        let callee_names: Vec<String> = link_plan
            .callees
            .iter()
            .filter_map(|symbol| match symbol {
                LinkSymbol::Function { item, .. } => {
                    beskid_analysis::resolve::qualified_name(&front.resolution, *item)
                }
                _ => None,
            })
            .collect();
        assert!(
            callee_names.iter().any(|name| name.contains("Esc")),
            "link plan callees should include Esc, got {callee_names:?}"
        );

        let artifact = lower_program_with_assembly_for_entrypoint(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&assembly),
            Some("ansi_csi_bold_red"),
        )
        .expect("lower ansi_csi_bold_red");
        validate_artifact(&artifact).expect("ansi_csi_bold_red link plan must validate");
    });
}
