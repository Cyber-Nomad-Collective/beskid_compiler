//! CLIF lowering gates for selected `corelib_tests` entries (per-test link plan).
//!
//! Run serially (lowering reuses assembly cache but holds process-global locks):
//! `cargo test -p beskid_tests_projects corelib_tests_codegen -- --nocapture --test-threads=1`

use crate::projects::fixture_harness::{
    corelib_tests_project_root, lower_corelib_tests_entrypoint, resolve_corelib_tests_entry_with_assembly,
    with_large_test_stack, with_project_test_env,
};

use std::collections::HashMap;
use std::sync::Arc;

use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeId, AstNodeKey, IndexedNodeKind, ItemSignature, SemanticTypeId, SourceUnitId, build_typed_program,
    child_nodes, generic_call_specialization, item_abi_signature, item_name, node_kind,
    project_session_for_syntax_assembly, reachable_items, with_db,
};

#[test]
fn syscall_result_predicates_have_call_derived_pointer_specializations() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("system/SystemSuite.bd");
            with_db(|db| {
                let syntax_assembly = Arc::new(resolved.assembly.as_ref().expect("corelib entry assembly").clone());
                let project = project_session_for_syntax_assembly(
                    db,
                    &syntax_assembly,
                    "syscall-result-specialization",
                    "prepared-syscall-result-specialization",
                )
                .expect("corelib syntax project session");
                let generation = syntax_assembly.generation;
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let entry = typed.assembly.entry_unit();
                let unit = SourceUnitId::new(db, entry.path.clone());
                let index = SyntaxIndex::from_program(&entry.program, generation);
                let expected =
                    ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL };
                // The suite merges several former targets; count only the four `Results.IsOk` calls of
                // the three syscall-write tests.
                let mut pending = index
                    .ids_of_kind(NodeKind::TestDefinition)
                    .map(|node| AstNodeKey { unit, generation, node })
                    .filter(|key| {
                        matches!(
                            item_name(db, *key).ok().flatten().as_deref(),
                            Some(
                                "syscall_write_empty_string_returns_non_negative"
                                    | "syscall_write_stdout_smoke"
                                    | "syscall_write_prints_newline"
                            )
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(pending.len(), 3, "the three syscall-write tests are in the system suite");
                let mut matching = 0;
                while let Some(key) = pending.pop() {
                    if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::CallExpression)
                        && generic_call_specialization(db, key)
                            .ok()
                            .flatten()
                            .is_some_and(|specialization| specialization.signature == expected)
                    {
                        matching += 1;
                    }
                    if let Some(children) = child_nodes(db, key).expect("child nodes") {
                        pending.extend(children.iter().copied());
                    }
                }

                assert_eq!(matching, 4, "each Results.IsOk call must retain POINTER -> BOOL");

                let entry = index
                    .ids_of_kind(NodeKind::TestDefinition)
                    .map(|node| AstNodeKey { unit, generation, node })
                    .find(|key| {
                        item_name(db, *key).ok().flatten().as_deref()
                            == Some("syscall_write_empty_string_returns_non_negative")
                    })
                    .expect("syscall test entrypoint");
                let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
                let reachable =
                    reachable_items(db, root, entry).expect("reachable item query").expect("reachable item facts");
                let mut specializations = HashMap::<AstNodeKey, Vec<ItemSignature>>::new();
                let mut pending = reachable.to_vec();
                while let Some(key) = pending.pop() {
                    if let Some(specialization) =
                        generic_call_specialization(db, key).expect("generic call specialization")
                    {
                        specializations.entry(specialization.declaration).or_default().push(specialization.signature);
                    }
                    if let Some(children) = child_nodes(db, key).expect("child nodes") {
                        pending.extend(children.iter().copied());
                    }
                }
                let missing = reachable
                    .iter()
                    .copied()
                    .filter(|key| {
                        node_kind(db, *key).ok().flatten() == Some(IndexedNodeKind::FunctionDefinition)
                            && item_abi_signature(db, *key).ok().flatten().is_none()
                            && !specializations.contains_key(key)
                    })
                    .map(|key| {
                        format!(
                            "{}@{:?}",
                            item_name(db, key).ok().flatten().unwrap_or_else(|| Arc::from("<anon>")),
                            key
                        )
                    })
                    .collect::<Vec<_>>();
                assert!(missing.is_empty(), "generic items without specialization: {missing:?}");
            });
        });
    });
}
