//! Prove System I/O smoke entries cannot use the retired HIR/`Lowerable` codegen drivers.

use std::path::PathBuf;
use std::sync::Arc;

use beskid_abi::{
    abi_v5::TargetMetadata,
    runtime_source::canonical_corelib_syscall_service_capability,
};
use beskid_analysis::services::{FrontEndOptions, ResolvedInput, resolve_input};
use beskid_codegen::RETIRED_HIR_LOWERING_PATH;
use beskid_codegen::lowering::lower_program_with_assembly_for_entrypoint;
use beskid_queries::{
    AstNodeId, AstNodeKey, CallLowering, SourceUnitId, SyntaxGenerationId,
    build_typed_program_with_corelib_syscall_services, call_lowering, child_nodes,
    compile_front_end_from_resolved_input, enum_constructor, item_name, node_kind, node_span,
    project_session_for_syntax_assembly, resolved_item, with_db,
};

fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("compiler workspace root")
        .to_path_buf()
}

fn assert_hir_driver_rejected(entry_rel: &str, entrypoint: &str) {
    let root = compiler_workspace_root();
    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src")
        .join(entry_rel);
    let project_root = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read entry");

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

    let errors = lower_program_with_assembly_for_entrypoint(
        &front.hir,
        &front.resolution,
        &front.typed,
        Some(&front.assembly),
        Some(entrypoint),
    )
    .expect_err("retired HIR driver must reject without fallback");
    let message = errors
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    assert!(
        message.contains(RETIRED_HIR_LOWERING_PATH),
        "entrypoint {entrypoint} in {entry_rel}: {message}"
    );
}

#[test]
fn lower_system_error_writeline_smoke_rejects_hir_driver() {
    assert_hir_driver_rejected("system/ErrorWriteTests.bd", "error_writeline_smoke");
}

#[test]
fn lower_system_input_read_smoke_rejects_hir_driver() {
    assert_hir_driver_rejected("system/InputReadTests.bd", "input_read_smoke");
}

#[test]
fn canonical_output_write_with_resolves_through_the_assembled_syntax_artifact() {
    let root = compiler_workspace_root();
    let entry = root.join("corelib/beskid_corelib/tests/corelib_tests/src/system/OutputWriteTests.bd");
    let project_root = entry
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("corelib test project root")
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read output test source");

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve corelib test workspace");
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
    .expect("resolved syntax frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    with_db(|db| {
        // Mirror the prepared-syntax boundary to inspect the exact canonical source facts before
        // attempting ISLE emission. This distinguishes a missing block rule from a missing
        // qualified-call declaration fact in the materialized Foundation source.
        let assembly = Arc::new(front.syntax_assembly());
        let generation = SyntaxGenerationId(1);
        let project = project_session_for_syntax_assembly(
            db,
            &assembly,
            "syntax-codegen",
            "prepared-frontend",
        )
        .expect("syntax project session");
        let manifest = beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(target.clone());
        let capability = canonical_corelib_syscall_service_capability(&manifest)
            .expect("canonical Corelib service capability");
        build_typed_program_with_corelib_syscall_services(
            db,
            project,
            generation,
            Arc::clone(&assembly),
            capability,
        )
        .expect("register canonical syntax facts");
        let output_root = AstNodeKey {
            unit: SourceUnitId::new(
                db,
                assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Core/Output/Output.bd"))
                    .expect("canonical Foundation Output unit")
                    .path
                    .clone(),
            ),
            generation,
            node: AstNodeId(0),
        };
        let mut pending = vec![output_root];
        let mut write_with = None;
        while let Some(key) = pending.pop() {
            let span = node_span(db, key).expect("node span");
            if node_kind(db, key).expect("node kind")
                == Some(beskid_queries::IndexedNodeKind::CallExpression)
                && span.is_some_and(|span| span.line_col_start.0 == 19)
            {
                write_with = Some(key);
                break;
            }
            if let Some(children) = child_nodes(db, key).expect("child nodes") {
                pending.extend(children.iter().copied());
            }
        }
        let write_with = write_with.expect("Core.Syscall.WriteWith call at Output.bd:19");
        let call_children = child_nodes(db, write_with)
            .expect("WriteWith call children")
            .expect("WriteWith call child list");
        let mut callee_nodes = vec![call_children[0]];
        let mut path = None;
        while let Some(key) = callee_nodes.pop() {
            if node_kind(db, key).expect("callee node kind")
                == Some(beskid_queries::IndexedNodeKind::PathExpression)
            {
                path = Some(key);
                break;
            }
            if let Some(children) = child_nodes(db, key).expect("callee child nodes") {
                callee_nodes.extend(children.iter().copied());
            }
        }
        let path = path.expect("qualified callee path");
        let path_resolution = resolved_item(db, path);
        let lowering = call_lowering(db, write_with);
        assert!(
            matches!(lowering, Ok(Some(CallLowering::Direct(_)))),
            "Core.Syscall.WriteWith must resolve to its assembled public function declaration; \\
             call_lowering={lowering:?}; resolved_item(callee_path)={path_resolution:?}"
        );

        let syscall_root = AstNodeKey {
            unit: SourceUnitId::new(
                db,
                assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Core/Syscall/Syscall.bd"))
                    .expect("canonical Foundation Syscall unit")
                    .path
                    .clone(),
            ),
            generation,
            node: AstNodeId(0),
        };
        let mut pending = vec![syscall_root];
        let mut syscall_write = None;
        while let Some(key) = pending.pop() {
            if node_kind(db, key).expect("Syscall node kind")
                == Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
                && item_name(db, key).expect("Syscall function name").as_deref() == Some("Write")
            {
                syscall_write = Some(key);
                break;
            }
            if let Some(children) = child_nodes(db, key).expect("Syscall child nodes") {
                pending.extend(children.iter().copied());
            }
        }
        let syscall_write = syscall_write.expect("Core.Syscall.Write function");
        let mut pending = vec![syscall_write];
        let mut invalid_fd = None;
        while let Some(key) = pending.pop() {
            let span = node_span(db, key).expect("Syscall node span");
            if node_kind(db, key).expect("Syscall node kind")
                == Some(beskid_queries::IndexedNodeKind::EnumConstructorExpression)
                && span.is_some_and(|span| span.line_col_start == (71, 16))
            {
                invalid_fd = Some(key);
                break;
            }
            if let Some(children) = child_nodes(db, key).expect("Syscall child nodes") {
                pending.extend(children.iter().copied());
            }
        }
        let invalid_fd = invalid_fd.expect("Result::Error(SyscallError::InvalidFd(fd))");
        let constructor = enum_constructor(db, invalid_fd);
        let layout = beskid_queries::enum_layout(db, invalid_fd);
        assert!(
            constructor.as_ref().ok().and_then(|fact| fact.as_ref()).is_some(),
            "Core.Syscall.Write must expose the Result::Error constructor fact for its fd guard; \
             constructor={constructor:?}; layout={layout:?}"
        );

    });
}
