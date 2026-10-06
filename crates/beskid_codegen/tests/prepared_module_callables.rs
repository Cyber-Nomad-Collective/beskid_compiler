//! Native Mod adapters must select the callable emitted from an exact registered key.
use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
use beskid_codegen::lower_prepared_syntax_module_with_callables;
use beskid_queries::{AstNodeKey, SemanticTypeId, SyntaxGenerationId, compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};

#[test]
fn prepared_module_issues_distinct_exact_callables_and_rejects_stale_keys() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Mod.bd");
    // Equal leaf names are deliberately distinct declarations and emitted functions.
    let source = "i32 Run(i32 value) { return value; } type Worker { i32 Run(i32 value) { return value; } }";
    std::fs::write(&path, source).unwrap();
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .unwrap();
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap();
    let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    let prepared =
        with_db(|db| lower_prepared_syntax_module_with_callables(db, &front, target.clone(), isa.as_ref())).unwrap();
    assert_eq!(prepared.target(), &target);
    assert_eq!(prepared.callables().len(), 2);
    let first = &prepared.callables()[0];
    let second = &prepared.callables()[1];
    assert_ne!(first.key(), second.key());
    assert_ne!(first.link_symbol(), second.link_symbol());
    for callable in prepared.callables() {
        assert_eq!(callable.signature().result, SemanticTypeId::I32);
        assert_eq!(callable.signature().parameters.last(), Some(&SemanticTypeId::I32));
        assert!(prepared.artifact().functions.iter().any(|function| function.name == callable.internal_symbol()));
        assert_eq!(prepared.callable(callable.key()).unwrap().link_symbol(), callable.link_symbol());
    }
    let stale = AstNodeKey { generation: SyntaxGenerationId(first.key().generation.0 + 1), ..first.key() };
    assert!(prepared.callable(stale).is_none(), "stale generation must not reuse a callable");
    let foreign_unit = with_db(|db| beskid_queries::SourceUnitId::new(db, directory.path().join("Foreign.bd")));
    assert!(prepared.callable(AstNodeKey { unit: foreign_unit, ..first.key() }).is_none());
    assert!(prepared.callable(AstNodeKey { node: beskid_queries::AstNodeId(u32::MAX), ..first.key() }).is_none());
}
