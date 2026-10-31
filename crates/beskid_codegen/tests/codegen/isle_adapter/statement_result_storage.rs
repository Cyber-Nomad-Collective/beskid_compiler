use super::support::{
    DirectCallee, HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, default_libcall_names,
    emit_isle_item, emit_isle_item_with_call_importer, find_function_definition, find_function_definitions,
    function_signature, item_fixture, item_fixture_with_root, item_name, types,
};

fn emit_main_with_imported_i64_callee(source: &str, callee_name: &str) -> String {
    let (input, isa, root) = item_fixture_with_root(source);
    let functions = find_function_definitions(input.database(), root);
    let callee = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(callee_name))
        .unwrap_or_else(|| panic!("{callee_name} definition"));
    let main = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I64, []);
    let imported = module
        .declare_function(callee_name, Linkage::Import, &signature)
        .unwrap_or_else(|error| panic!("declare {callee_name} import: {error}"));
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(callee), imported)]));

    match emit_isle_item_with_call_importer(&input, isa.as_ref(), main, &mut importer) {
        Ok(function) => function.display().to_string(),
        Err(error) => {
            panic!("Main must lower with its {callee_name} import: {}", error.display_with_db(input.database()))
        }
    }
}

fn assert_direct_call_result_returns(clif: &str) {
    assert!(clif.lines().any(|line| line.contains(" = call ")), "{clif}");
    assert!(!clif.contains("call_indirect"), "{clif}");
    assert!(clif.lines().any(|line| line.trim_start().starts_with("return ")), "{clif}");
}

#[test]
fn nested_direct_call_results_lower_through_exact_statement_facts() {
    let clif = emit_main_with_imported_i64_callee(
        "i64 Count() { return 1_i64; } i64 Forward() { return Count(); } i64 Main() { i64 value = Forward(); return value; }",
        "Forward",
    );

    assert_direct_call_result_returns(&clif);
}

#[test]
fn inferred_let_results_lower_through_canonical_storage_facts() {
    let clif = emit_main_with_imported_i64_callee(
        "i64 Count() { return 1_i64; } i64 Main() { let value = Count(); return value; }",
        "Count",
    );

    assert_direct_call_result_returns(&clif);
}

#[test]
fn narrower_call_results_widen_at_the_declared_local_storage_boundary() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Count() { return 1; } i64 Main() { i64 value = Count(); return value; }");
    let functions = find_function_definitions(input.database(), root);
    let callee = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Count"))
        .expect("Count definition");
    let caller = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I32, []);
    let imported = module.declare_function("Count", Linkage::Import, &signature).expect("declare Count import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(callee), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("an authorized narrower integer result must widen at explicit local storage");
    let clif = function.display().to_string();

    assert!(clif.contains("sextend.i64"), "{clif}");
}

#[test]
fn unsigned_byte_call_results_zero_extend_at_the_declared_local_storage_boundary() {
    let (input, isa, root) =
        item_fixture_with_root("u8 Byte() { return 255_u8; } i64 Main() { i64 value = Byte(); return value; }");
    let functions = find_function_definitions(input.database(), root);
    let callee = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Byte"))
        .expect("Byte definition");
    let caller = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I8, []);
    let imported = module.declare_function("Byte", Linkage::Import, &signature).expect("declare Byte import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(callee), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("an unsigned byte result must widen at explicit local storage");
    let clif = function.display().to_string();

    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(!clif.contains("sextend.i64"), "{clif}");
}

#[test]
fn unsigned_byte_call_results_zero_extend_at_the_return_boundary() {
    let (input, isa, root) = item_fixture_with_root("u8 Byte() { return 255_u8; } i64 Main() { return Byte(); }");
    let functions = find_function_definitions(input.database(), root);
    let callee = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Byte"))
        .expect("Byte definition");
    let caller = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I8, []);
    let imported = module.declare_function("Byte", Linkage::Import, &signature).expect("declare Byte import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(callee), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("an unsigned byte result must widen at the function return boundary");
    let clif = function.display().to_string();

    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(!clif.contains("sextend.i64"), "{clif}");
}

#[test]
fn unsigned_byte_block_results_zero_extend_at_the_contextual_block_boundary() {
    let (input, isa, root) =
        item_fixture_with_root("u8 Byte() { return 255_u8; } i64 Main() { i64 value = { Byte(); }; return value; }");
    let functions = find_function_definitions(input.database(), root);
    let callee = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Byte"))
        .expect("Byte definition");
    let caller = functions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I8, []);
    let imported = module.declare_function("Byte", Linkage::Import, &signature).expect("declare Byte import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(callee), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("an unsigned byte block tail must widen to its contextual block type");
    let clif = function.display().to_string();

    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(!clif.contains("sextend.i64"), "{clif}");
}

#[test]
fn block_expression_partitions_prefix_statements_from_its_final_value() {
    let (input, isa, item) = item_fixture("i64 Main() { i64 value = { i64 nested = 1_i64; nested; }; return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a block-valued local must lower its prefix statements and final expression exactly once");
    let clif = function.display().to_string();

    assert!(clif.contains("iconst.i64 1"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn scalar_match_results_lower_at_return_and_typed_storage_boundaries() {
    let (input, isa, item) = item_fixture(
        "enum Choice { First, Second } i64 Main(Choice choice) { i64 selected = match choice { Choice::First => 1_i64, Choice::Second => 2_i64, }; return match choice { Choice::First => selected, Choice::Second => 0_i64, }; }",
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("match results must lower through exact return and storage facts");
    let clif = function.display().to_string();

    assert_eq!(clif.matches("brif").count(), 4, "two ordered two-arm matches must retain four tag tests:\n{clif}");
    assert_eq!(clif.matches("load.i32").count(), 4, "each ordered tag test must load the enum tag:\n{clif}");
    for value in ["iconst.i64 0", "iconst.i64 1", "iconst.i64 2"] {
        assert!(clif.contains(value), "match scalar result `{value}` missing:\n{clif}");
    }
    assert!(clif.lines().any(|line| line.trim_start().starts_with("return ")), "{clif}");
}

#[test]
fn mutable_scalar_assignment_lowers_only_with_matching_storage_type() {
    let (input, isa, item) = item_fixture("i64 Main() { mut i64 value = 0_i64; value = 1_i64; return value; }");

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("matching mutable-local storage must lower through its exact slot and ABI fact");
}

#[test]
fn immutable_assignment_fails_before_clif_emission() {
    let (input, isa, root) = item_fixture_with_root("i64 Main() { i64 value = 0_i64; value = 1_i64; return value; }");
    let item = find_function_definition(input.database(), root).expect("Main definition");

    let error = emit_isle_item(&input, isa.as_ref(), item)
        .expect_err("immutable assignment must not acquire storage authority");
    let rendered = error.display_with_db(input.database());

    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("AssignExpression@"), "{rendered}");
}
