//! Core.Parallel runtime builtins (`__isolate_*`, `__port_*`) in every expression position.
//!
//! Corelib service authority is bound to the exact compiler-owned source bytes, so these tests
//! lower the canonical Core.Parallel items themselves: struct literal fields (`FailureOf`,
//! `Current`, `OfTask`), a return through a comparison (`IsCancelled`), and conditions and
//! call arguments (`Scope.Finish`).

use super::support::{
    AstNodeId, AstNodeKey, CodegenInput, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem,
    canonical_corelib_project_fixture, find_corelib_service_call, lower_syntax_program, named_function,
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
