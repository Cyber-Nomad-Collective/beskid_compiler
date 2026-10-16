//! Core.Parallel runtime builtins (`__isolate_*`, `__port_*`) in every expression position.
//!
//! Corelib service authority is bound to the exact compiler-owned source bytes, so these tests
//! lower the canonical Core.Parallel items themselves: struct literal fields (`FailureOf`,
//! `Current`, `OfTask`), a return through a comparison (`IsCancelled`), and a typed value
//! service feeding a local of each `Task<T>.Join` specialization.

use super::support::{
    AstNodeId, AstNodeKey, CodegenInput, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem,
    canonical_corelib_project_fixture, find_corelib_service_call, find_function_definitions, find_nodes_of_kind,
    item_name, lower_syntax_program, named_function,
};

fn parallel_unit(input: &CodegenInput<'_>, entry: AstNodeKey, relative: &str) -> AstNodeKey {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/concurrency/src");
    let path = std::fs::canonicalize(root).expect("Concurrency source root").join(relative);
    AstNodeKey { unit: SourceUnitId::new(input.database(), path), generation: entry.generation, node: AstNodeId(0) }
}

#[test]
fn isolate_builtins_lower_as_struct_literal_fields_and_returns() {
    let (input, isa, entry) = canonical_corelib_project_fixture(
        &[(
            "Main.bd",
            "use Core.Parallel.Scopes; use Core.Parallel.Isolate; use Core.Parallel; i64 Main() { return 0_i64; }",
        )],
        SyntaxGenerationId(141),
    );
    let scopes = parallel_unit(&input, entry, "Core/Parallel/Scopes.bd");
    let isolate = parallel_unit(&input, entry, "Core/Parallel/Isolate.bd");
    let parallel = parallel_unit(&input, entry, "Core/Parallel/Parallel.bd");
    let failure_of = named_function(&input, scopes, "FailureOf");

    // `string __isolate_join_message(i64)` answers through the pointer register; the source
    // declaration, not that transport, makes the call a managed value that a struct field roots.
    let message = find_corelib_service_call(input.database(), failure_of, "__isolate_join_message")
        .expect("FailureOf reads the failure message");
    assert_eq!(
        beskid_queries::managed_reference_kind(input.database(), message).expect("message category"),
        Some(beskid_queries::ManagedReferenceKind::GcManaged)
    );
    let index = find_corelib_service_call(input.database(), failure_of, "__isolate_task_index")
        .expect("FailureOf reads the task index");
    assert_eq!(
        beskid_queries::managed_reference_kind(input.database(), index).expect("index category"),
        Some(beskid_queries::ManagedReferenceKind::NativeOrScalar)
    );

    let items = [
        (failure_of, "FailureOf"),
        (named_function(&input, isolate, "Current"), "Current"),
        (named_function(&input, isolate, "Main"), "Main"),
        (named_function(&input, isolate, "OfTask"), "OfTask"),
        (named_function(&input, parallel, "IsCancelled"), "IsCancelled"),
    ]
    .map(|(key, symbol)| SyntaxModuleItem { key, symbol: symbol.into() });
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("Core.Parallel runtime builtins lower in struct literal fields and returns");
    for symbol in ["isolate_join_message", "isolate_task_index", "isolate_current", "isolate_task_isolate"] {
        assert!(
            artifact.extern_imports.iter().any(|import| import.symbol == symbol),
            "missing import {symbol}: {:?}",
            artifact.extern_imports.iter().map(|import| import.symbol.as_str()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn isolate_join_value_lowers_into_a_typed_local_of_each_task_specialization() {
    // `T value = __isolate_join_value<T>(this.id);` in `Task<T>.Join`: the typed value service
    // feeds a local whose declared type is the enclosing generic, per call-derived instance.
    let (input, isa, entry) = canonical_corelib_project_fixture(
        &[(
            "Main.bd",
            "use Core.Parallel.Scopes; use Core.Results; \
             i64 TakeNumber(Task<i64> task) { return match task.Join() { Result::Ok(value) => value, Result::Error(_) => 0_i64, }; } \
             string TakeText(Task<string> task) { return match task.Join() { Result::Ok(value) => value, Result::Error(_) => \"\", }; } \
             bool TakeFlag(Task<bool> task) { return match task.Join() { Result::Ok(value) => value, Result::Error(_) => false, }; }",
        )],
        SyntaxGenerationId(142),
    );
    let scopes = parallel_unit(&input, entry, "Core/Parallel/Scopes.bd");
    let join = find_nodes_of_kind(input.database(), scopes, beskid_queries::IndexedNodeKind::MethodDefinition)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Join"))
        .expect("Task.Join");
    let mut items = vec![
        SyntaxModuleItem { key: join, symbol: "Task_Join".into() },
        SyntaxModuleItem { key: named_function(&input, scopes, "FailureOf"), symbol: "FailureOf".into() },
        SyntaxModuleItem { key: named_function(&input, scopes, "StatusCompleted"), symbol: "StatusCompleted".into() },
        SyntaxModuleItem { key: named_function(&input, scopes, "StatusCancelled"), symbol: "StatusCancelled".into() },
        SyntaxModuleItem { key: named_function(&input, scopes, "StatusPanicked"), symbol: "StatusPanicked".into() },
    ];
    items.extend(find_function_definitions(input.database(), entry).into_iter().map(|key| SyntaxModuleItem {
        key,
        symbol: item_name(input.database(), key).ok().flatten().expect("named entry item").to_string(),
    }));
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("Task<T>.Join lowers its typed join value for scalar, bool and managed T");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names.iter().filter(|name| name.starts_with("Task_Join#generic_")).count(), 3, "{names:?}");
    assert!(artifact.extern_imports.iter().any(|import| import.symbol == "isolate_join_value"));
}
