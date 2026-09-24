//! Parsed test programs and statement matches lowered without HIR.

use super::super::support::{
    Arc, DirectCallee, HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, NodeFacts,
    SyntaxModuleItem, call_abi_signature, call_lowering, default_libcall_names, emit_isle_item,
    emit_isle_item_with_call_importer, find_call_expression, find_function_definition, find_function_definitions,
    find_nodes_of_kind, find_test_definition, item_body, item_fixture, item_fixture_with_root, item_name,
    lower_syntax_program,
};

#[test]
fn parsed_test_definition_with_result_match_binding_lowers_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } test sample { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0_i64 { return; } }, Result::Error(_) => {}, }; }",
    );
    let test_item = find_test_definition(input.database(), root).expect("test item");
    let function = match emit_isle_item(&input, isa.as_ref(), test_item) {
        Ok(function) => function,
        Err(error) => {
            panic!("TestDefinition with Ok(written) match must lower: {}", error.display_with_db(input.database()))
        }
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn parsed_generic_result_match_with_nominal_error_binds_ok_payload_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum SyscallError { InvalidFd(i64 fd), IoFailure(i64 code) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(7_i64); match result { Result::Ok(written) => { written; }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "nominal Error payload + Ok(written) binding must lower: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn parsed_generic_result_match_arm_uses_bound_payload_in_comparison_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0_i64 { return; } }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => {
            panic!("bound payload comparison inside match arm must lower: {}", error.display_with_db(input.database()))
        }
    };
    let clif = function.display().to_string();
    assert!(clif.contains("icmp"), "{clif}");
}

#[test]
fn parsed_generic_enum_match_statement_binds_scalar_payload_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { written; }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "generic result statement match must bind Ok(written) payload: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("load.i64") || clif.contains("load"), "{clif}");
}

#[test]
fn parsed_generic_enum_match_statement_lowers_empty_unit_blocks_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, string> result = Result<i64, string>::Ok(7_i64); match result { Result::Ok(_) => {}, Result::Error(_) => {}, }; return; }",
    );
    let body = item_body(input.database(), item).expect("item body query").expect("item body");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.statement_count(body), Some(3), "function body statements");

    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "generic result statement match lowers empty unit arm blocks: {}",
            error.display_with_db(input.database())
        ),
    };

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn parsed_statement_match_lowers_empty_blocks_and_the_final_effect_in_unit_blocks() {
    let (input, isa, item) = item_fixture(
        "enum Result { Ok, Error } unit Main() { mut i64 observed = 0_i64; Result result = Result::Ok; match result { Result::Ok => { observed = 1_i64; }, Result::Error => {}, }; return; }",
    );

    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "statement-position match blocks must lower all effects and accept an empty arm: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();

    assert!(clif.contains("iconst.i64 1"), "the final arm effect must not be withheld as a value: {clif}");
}

#[test]
fn parsed_generic_enum_match_statement_lowers_direct_unit_call_arms_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Fail() { return; } unit Main() { Result<i64, i64> result = Result<i64, i64>::Error(0_i64); match result { Result::Ok(_) => {}, Result::Error(_) => Fail(), }; return; }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let fail = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Fail"))
        .expect("Fail item");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main item");
    let fail_call = find_nodes_of_kind(db, main, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|key| {
            matches!(
                call_lowering(db, *key).ok().flatten(),
                Some(beskid_queries::CallLowering::Direct(declaration)) if declaration == fail
            )
        })
        .expect("direct Fail arm call");
    assert_eq!(
        beskid_codegen::SyntaxNodeFacts::new(&input).direct_callee(fail_call),
        Some(DirectCallee::item(fail)),
        "the match arm must retain its direct unit callee"
    );

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    let imported = module.declare_function("Fail", Linkage::Import, &signature).expect("declare imported unit callee");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(fail), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), main, &mut importer)
        .expect("direct unit call arm lowers through the match statement path");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "{clif}");
}

#[test]
fn parsed_test_program_specializes_is_ok_and_binds_match_payload_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } unit True(bool condition, string because) { if condition { return; } return; } test sample { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); True(IsOk(result), \"ok\"); match result { Result::Ok(written) => { True(written >= 0_i64, \"nonneg\"); }, Result::Error(_) => {}, }; }",
    );
    let db = input.database();
    let is_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk");
    let true_fn = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("True"))
        .expect("True");
    let test = find_test_definition(db, root).expect("test");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: true_fn, symbol: "True".into() },
            SyntaxModuleItem { key: test, symbol: "sample".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("SyscallWrite-shaped IsOk + Ok(written) test must lower: {error:?}"),
    };
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("IsOk#generic_")),
        "IsOk must specialize: {:?}",
        artifact.functions.iter().map(|function| &function.name).collect::<Vec<_>>(),
    );
}

#[test]
fn parsed_test_program_specializes_a_generic_call_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } test Main { string value = \"same\"; Equal(value, value, \"because\"); }",
    );
    let generic = find_function_definition(input.database(), root).expect("generic function");
    let test = find_test_definition(input.database(), root).expect("test item");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: generic, symbol: "Equal".into() },
            SyntaxModuleItem { key: test, symbol: "Main".into() },
        ],
    )
    .expect("test-body generic calls produce exact syntax ABI specializations");

    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "test-body generic calls must emit their exact specialization",
    );
}

#[test]
fn parsed_test_program_lowers_a_bare_i64_generic_argument_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i64 Position() { return 0_i64; } unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } test Main { Equal(Position(), 0, \"initial position\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let test = find_test_definition(input.database(), root).expect("test item");
    let call = find_call_expression(input.database(), test).expect("outer Equal call");
    assert_eq!(
        call_abi_signature(input.database(), call).expect("generic call signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([
                beskid_queries::SemanticTypeId::I64,
                beskid_queries::SemanticTypeId::I64,
                beskid_queries::SemanticTypeId::STRING,
            ]),
            result: beskid_queries::SemanticTypeId::UNIT,
        }),
    );

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Position".into() },
            SyntaxModuleItem { key: items[1], symbol: "Equal".into() },
            SyntaxModuleItem { key: test, symbol: "Main".into() },
        ],
    )
    .expect("syntax lowering keeps the generic literal at the specialized ABI width");

    beskid_codegen::validate_artifact(&artifact).expect("generic artifact is ABI-valid");
    let equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Equal#generic_"))
        .expect("specialized Equal function");
    let clif = equal.function.display().to_string();
    assert!(clif.contains("i64"), "{clif}");
}

#[test]
fn cyb137_bound_payload_compare_unsuffixed_integer_must_lower() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0 { return; } }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!("CYB-137 unsuffixed compare must lower: {}", error.display_with_db(input.database())),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("icmp"), "{clif}");
}

#[test]
fn cyb137_assert_true_is_ok_then_bound_payload_match_must_lower() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } enum SyscallError { InvalidFd(i64 fd) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } unit True(bool condition, string because) { if condition { return; } return; } test sample { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(0_i64); True(IsOk(result), \"ok\"); match result { Result::Ok(written) => { True(written >= 0, \"nonneg\"); }, Result::Error(_) => {}, }; }",
    );
    let db = input.database();
    let is_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk");
    let true_fn = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("True"))
        .expect("True");
    let test = find_test_definition(db, root).expect("test");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: true_fn, symbol: "True".into() },
            SyntaxModuleItem { key: test, symbol: "sample".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("CYB-137 SyscallWrite-shaped fixture must lower: {error:?}"),
    };
    assert!(
        artifact.functions.iter().any(|f| f.name.starts_with("IsOk#generic_")),
        "IsOk specialization missing: {:?}",
        artifact.functions.iter().map(|f| &f.name).collect::<Vec<_>>(),
    );
}

#[test]
fn cyb169_enum_return_i64_main_must_lower() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result { Ok(i64 value), Error(i64 error) } Result MakeOk() { return Result::Ok(7_i64); } i64 Main() { Result result = MakeOk(); return match result { Result::Ok(value) => value, Result::Error(_) => -1_i64, }; }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let make_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("MakeOk"))
        .expect("MakeOk");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: make_ok, symbol: "MakeOk".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("CYB-169 enum return with i64 Main must lower: {error:?}"),
    };
    assert!(artifact.functions.iter().any(|f| f.name.contains("Main")), "Main missing from artifact");
}
