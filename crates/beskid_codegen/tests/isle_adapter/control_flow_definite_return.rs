//! Agreement of the definite-return check (E1258) with lowering: every valued body the legality
//! gate accepts ends its final block, so lowering never reports "generated statement body did
//! not terminate its final block" (E2103) for it.

use super::support::{
    HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, call_lowering, default_libcall_names,
    emit_isle_item, emit_isle_item_with_call_importer, find_call_expression, find_function_definitions,
    item_fixture, item_fixture_with_root,
};

fn assert_accepted_body_lowers(source: &str) {
    let (input, isa, item) = item_fixture(source);
    let findings = beskid_queries::check_items(input.database(), &[item]).err().unwrap_or_default();
    assert!(
        !findings.iter().any(|finding| finding.kind.code() == "E1258"),
        "the definite-return check must accept {source}: {findings:?}"
    );
    if let Err(error) = emit_isle_item(&input, isa.as_ref(), item) {
        panic!("an accepted valued body must lower: {source}: {error}");
    }
}

#[test]
fn every_body_the_definite_return_check_accepts_lowers() {
    for source in [
        "i32 F(bool c) { if (c) { return 1; } else { return 2; } }",
        "i32 F(bool c, bool d) { if (c) { return 1; } else if (d) { return 2; } else { return 3; } }",
        "i32 F() { while (true) { } }",
        "i32 F(i32 x) { while (true) { if (x > 0) { return x; } } }",
        "i32 F() { while (true) { while (true) { break; } } }",
        "enum Step { Stop(), Go(i32 value) } \
         i32 F(Step s) { match s { Step::Stop() => { return 0; }, Step::Go(v) => { return v; }, } }",
        "i32 F(i32 x) { match x { 1 => { return 1; }, _ => { return 0; }, } }",
        "i32 F() { { return 1; }; }",
        "i32 F(bool c) { { if (c) { return 1; } }; return 2; }",
    ] {
        assert_accepted_body_lowers(source);
    }
}

#[test]
fn a_body_that_falls_off_its_end_is_rejected_before_lowering() {
    let (input, _isa, item) = item_fixture("i32 Main() { 0; }");
    let findings = beskid_queries::check_items(input.database(), &[item]).expect_err("the gate rejects Main");
    assert_eq!(findings.iter().map(|finding| finding.kind.code()).collect::<Vec<_>>(), ["E1258"]);
}

#[test]
fn a_final_call_that_returns_never_lowers_to_a_trap() {
    let (input, isa, root) = item_fixture_with_root("never Stop() { while (true) { } } i32 Main() { Stop(); }");
    let db = input.database();
    let items = find_function_definitions(db, root);
    let caller = items[1];
    let findings = beskid_queries::check_items(db, &[caller]).err().unwrap_or_default();
    assert!(!findings.iter().any(|finding| finding.kind.code() == "E1258"), "{findings:?}");
    let call = find_call_expression(db, caller).expect("call syntax key");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(db, call).expect("direct-call query").expect("direct call")
    else {
        panic!("expected a direct call");
    };
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    let imported = module.declare_function("Stop", Linkage::Import, &signature).expect("declare Stop");
    let mut importer =
        ItemModuleImporter::new(&mut module, HashMap::from([(beskid_isle::DirectCallee::item(declaration), imported)]));
    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .unwrap_or_else(|error| panic!("a final call that returns never must lower: {error}"));
    let clif = function.display().to_string();
    assert!(clif.contains("trap"), "the call that returns never is followed by a trap: {clif}");
}
