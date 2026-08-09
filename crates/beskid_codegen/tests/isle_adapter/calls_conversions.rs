use super::support::{
    Arc, CANONICAL_CORELIB_ARGS_SOURCE_PATH, CorelibServiceImportFacts, DirectCallee, FunctionEmitter, HashMap,
    ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, NodeFacts, SyntaxModuleItem, UserFuncName,
    assert_args_module_cannot_emit_imports, call_abi_signature, call_lowering, canonical_corelib_service_source_path,
    canonical_corelib_service_sources, canonical_corelib_syscall_fixture, canonical_foundation_assert_fixture,
    canonical_foundation_error_fixture, canonical_foundation_output_fixture, core_args_fixture, default_libcall_names,
    emit_isle_item, emit_isle_item_with_call_importer, find_call_expression, find_corelib_service_call,
    find_function_definitions, function_signature, item_body, item_fixture, item_fixture_with_root, item_name,
    lower_syntax_program, materialized_corelib_syscall_fixture, named_function, types,
};

#[test]
fn parsed_i32_variable_assignment_to_i64_local_requires_explicit_conversion() {
    let (input, isa, root) = item_fixture_with_root(
        "i64 Main() { mut i32 source = 0; mut i64 destination = 0_i64; destination = source; return destination; }",
    );
    let item = named_function(&input, root, "Main");

    let error = emit_isle_item(&input, isa.as_ref(), item)
        .expect_err("an i32 variable must not implicitly widen during mutable i64 assignment");
    let rendered = error.display_with_db(input.database());

    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("AssignExpression@"), "{rendered}");
}

#[test]
fn parsed_i32_to_i64_conversion_lowers_as_a_typed_let_initializer() {
    let (input, isa, root) = item_fixture_with_root("i64 Main(i32 width) { i64 count = i64(width); return count; }");
    let item = named_function(&input, root, "Main");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("an explicit i32-to-i64 conversion must lower as a typed let initializer");
    let clif = function.display().to_string();

    assert!(clif.contains("sextend.i64"), "{clif}");
    assert!(clif.lines().any(|line| line.trim_start().starts_with("return ")), "{clif}");
}

#[test]
fn parsed_u8_to_i64_conversion_lowers_as_a_direct_return() {
    let (input, isa, root) = item_fixture_with_root("i64 Main(u8 value) { return i64(value); }");
    let item = named_function(&input, root, "Main");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("an explicit u8-to-i64 conversion result must retain its destination type");
    let clif = function.display().to_string();

    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(clif.lines().any(|line| line.trim_start().starts_with("return ")), "{clif}");
}

#[test]
fn parsed_u8_to_i64_conversion_lowers_as_a_direct_call_argument() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
string Digit(i64 value) { return "x"; }
string Main(u8 value) { return Digit(i64(value)); }
"#,
    );
    let functions = find_function_definitions(input.database(), root);
    let items = [
        SyntaxModuleItem { key: functions[0], symbol: "Digit".into() },
        SyntaxModuleItem { key: functions[1], symbol: "Main".into() },
    ];

    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("an explicit numeric conversion result must retain its type as a direct call argument");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main function");
    assert!(main.function.display().to_string().contains("uextend.i64"));
}

#[test]
fn parsed_string_interpolation_coerces_i64_operand() {
    let (input, isa, root) = item_fixture_with_root(r#"string Main(i64 value) { return "value=${value}"; }"#);
    let item = named_function(&input, root, "Main");

    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: item, symbol: "Main".into() }])
        .expect("numeric interpolation must lower through syntax ISLE string coercion");
    let clif = artifact.functions[0].function.display().to_string();

    assert!(
        clif.contains(&format!("iconst.i32 {}", beskid_abi::TAG_STR_FROM_I64)),
        "numeric formatting dispatch must be imported: {clif}",
    );
    assert!(
        clif.contains(&format!("iconst.i32 {}", beskid_abi::TAG_STR_CONCAT)),
        "string concatenation dispatch must be imported: {clif}",
    );
}

#[test]
fn parsed_pointer_signature_uses_the_target_pointer_type_without_hir() {
    let (input, isa, item) = item_fixture("pointer Echo(pointer value) { return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item).expect("pointer syntax lowers through generated ISLE");
    let clif = function.display().to_string();
    assert!(clif.contains("function u0:0(i64) -> i64"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn parsed_generic_nominal_aggregate_uses_its_source_proven_pointer_abi_signature() {
    // Generic Create<T> has no fixed item ABI; the call site must prove Channel<i64> -> POINTER.
    let (input, isa, root) = item_fixture_with_root(
        "type Channel<T> { i64 handle } Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; } unit Main() { Channel<i64> ch = Create<i64>(); return; }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let create = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Create"))
        .expect("Create");
    let main =
        items.iter().copied().find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main")).expect("Main");
    assert_eq!(
        beskid_queries::item_abi_signature(db, create).expect("generic item ABI query"),
        None,
        "generic factories must not expose a fixed item ABI"
    );
    let call = find_call_expression(db, main).expect("Create call");
    let signature =
        call_abi_signature(db, call).expect("specialized call ABI query").expect("Create<i64> specialization");
    assert_eq!(signature.result, beskid_queries::SemanticTypeId::POINTER);
    let _ = isa;
}

#[test]
fn parsed_direct_call_uses_explicit_item_module_importer() {
    let (input, isa, root) =
        item_fixture_with_root("i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }");
    let db = input.database();
    let items = find_function_definitions(db, root);
    let callee = items[0];
    let caller = items[1];
    let call = find_call_expression(db, caller).expect("call syntax key");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(db, call).expect("direct-call query").expect("direct call")
    else {
        panic!("expected a syntax-resolved direct call");
    };
    assert_eq!(declaration, callee);

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I32, [types::I32]);
    let imported =
        module.declare_function("AddOne", Linkage::Import, &signature).expect("declare imported syntax item");
    let mut importer =
        ItemModuleImporter::new(&mut module, HashMap::from([(beskid_isle::DirectCallee::item(declaration), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("parsed direct call lowers through explicit module import");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "{clif}");
    assert!(clif.contains("iconst.i32 41"), "{clif}");
}

#[test]
fn canonical_corelib_service_call_imports_its_distinct_abi_symbol() {
    let (input, isa, root) = canonical_corelib_syscall_fixture();
    let read = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Read"))
        .expect("Core.Syscall Read source item");
    let call = find_corelib_service_call(input.database(), read, "__syscall_read").expect("__syscall_read call");
    let service = DirectCallee::corelib_service("syscall_read");
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), isa.pointer_type(), [types::I64, types::I64]);
    let imported = module
        .declare_function("syscall_read", Linkage::Import, &signature)
        .expect("declare the exact Corelib service import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(service.clone(), imported)]));
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.direct_callee(call), Some(service.clone()));
    assert_eq!(
        call_abi_signature(input.database(), call).expect("Corelib service ABI fact"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([beskid_queries::SemanticTypeId::I64, beskid_queries::SemanticTypeId::I64,]),
            result: beskid_queries::SemanticTypeId::STRING,
        })
    );

    let service_facts = CorelibServiceImportFacts::new(input.database(), service);

    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 91),
            emitter.signature([], [isa.pointer_type()]),
            &service_facts,
            service_facts.call,
            &mut importer,
        )
        .expect("compiler-authorized Corelib service lowers through an exact import");
    assert!(function.display().to_string().contains("call"));
}

#[test]
fn materialized_foundation_syscall_facade_imports_its_authorized_write_service() {
    let (input, isa, root) = materialized_corelib_syscall_fixture();
    let write = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Write"))
        .expect("materialized Core.Syscall Write source item");
    let call = find_corelib_service_call(input.database(), write, "__syscall_write")
        .expect("materialized __syscall_write call");
    assert_eq!(
        beskid_codegen::SyntaxNodeFacts::new(&input).direct_callee(call),
        Some(DirectCallee::corelib_service("syscall_write")),
        "only loader-proven materialized Foundation source receives the service fact"
    );
    let service = DirectCallee::corelib_service("syscall_write");
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I64, [types::I64, isa.pointer_type()]);
    let imported = module
        .declare_function("syscall_write", Linkage::Import, &signature)
        .expect("declare materialized Corelib service import");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(service.clone(), imported)]));
    let service_facts = CorelibServiceImportFacts::new(input.database(), service);
    let function = FunctionEmitter::new(isa.as_ref())
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 97),
            FunctionEmitter::new(isa.as_ref()).signature([], [types::I64]),
            &service_facts,
            service_facts.call,
            &mut importer,
        )
        .expect("materialized Core.Syscall call lowers through the authorized external import");
    assert!(
        function.display().to_string().contains("call"),
        "the trusted materialized facade must import syscall_write"
    );
}

#[test]
fn canonical_foundation_args_module_emits_only_the_authorized_args_imports() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded Core.Args source");
    let source_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("compiler-owned Core.Args path");
    let (input, isa, root) = core_args_fixture(source_path, source.source, Arc::from([]));

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: named_function(&input, root, "ProgramName"), symbol: "ProgramName".into() }],
    )
    .expect("canonical Core.Args module emits through syntax ISLE");
    let mut imports = artifact.extern_imports.iter().map(|import| import.symbol.as_str()).collect::<Vec<_>>();
    imports.sort_unstable();
    assert_eq!(
        imports,
        vec!["args_count", "args_get"],
        "the canonical Core.Args module is the sole source authorized to import both ABI services"
    );
}

#[test]
fn copied_materialized_foundation_args_module_cannot_emit_args_imports() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded Core.Args source");
    let directory = tempfile::tempdir().expect("copied materialized Core.Args project").keep();
    let source_path = directory.join("obj/beskid/deps/src/foundation/Core/Args/Args.bd");
    std::fs::create_dir_all(source_path.parent().expect("Core.Args parent")).expect("create Core.Args parent");
    std::fs::write(&source_path, &source.source).expect("write copied Core.Args source");
    let (input, isa, root) = core_args_fixture(source_path.clone(), source.source, Arc::from([source_path]));

    assert_args_module_cannot_emit_imports(&input, isa.as_ref(), root);
}

#[cfg(unix)]
#[test]
fn symlinked_expected_materialized_foundation_args_module_cannot_emit_args_imports() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded Core.Args source");
    let directory = tempfile::tempdir().expect("symlinked materialized Core.Args project").keep();
    let target_path = directory.join("target/Args.bd");
    let source_path = directory.join("obj/beskid/deps/src/foundation/Core/Args/Args.bd");
    std::fs::create_dir_all(target_path.parent().expect("target parent")).expect("create target parent");
    std::fs::create_dir_all(source_path.parent().expect("Core.Args parent")).expect("create Core.Args parent");
    std::fs::write(&target_path, &source.source).expect("write symlink target");
    std::os::unix::fs::symlink(&target_path, &source_path).expect("link expected materialized Core.Args path");
    let (input, isa, root) = core_args_fixture(source_path.clone(), source.source, Arc::from([source_path]));

    assert_args_module_cannot_emit_imports(&input, isa.as_ref(), root);
}

#[test]
fn altered_foundation_args_module_cannot_emit_args_imports() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded Core.Args source");
    let source_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("compiler-owned Core.Args path");
    let altered = format!("{}\n// altered", source.source);
    let (input, isa, root) = core_args_fixture(source_path, altered, Arc::from([]));

    assert_args_module_cannot_emit_imports(&input, isa.as_ref(), root);
}

#[test]
fn user_args_named_module_cannot_emit_args_imports() {
    let directory = tempfile::tempdir().expect("user Core.Args project").keep();
    let source_path = directory.join("Core/Args/Args.bd");
    let source =
        "pub string ProgramName() { i64 count = __args_count(); if count < 1 { return \"\"; } return __args_get(0); }";
    std::fs::create_dir_all(source_path.parent().expect("Core.Args parent")).expect("create Core.Args parent");
    std::fs::write(&source_path, source).expect("write user Core.Args source");
    let (input, isa, root) = core_args_fixture(source_path, source.into(), Arc::from([]));

    assert_args_module_cannot_emit_imports(&input, isa.as_ref(), root);
}

#[test]
fn canonical_foundation_assert_trigger_failure_lowers_only_the_panic_service() {
    let (input, isa, root) = canonical_foundation_assert_fixture();
    let trigger_failure = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("trigger_failure"))
        .expect("canonical Assert trigger_failure");
    let call = find_corelib_service_call(input.database(), trigger_failure, "__panic_str")
        .expect("canonical Assert panic call");
    assert!(matches!(
        call_lowering(input.database(), call).expect("panic lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__panic_str" && service.symbol == "panic_str"
    ));

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: trigger_failure, symbol: "trigger_failure".into() }],
    )
    .expect("canonical Assert lowers through syntax ISLE");
    let imports = &artifact.extern_imports;
    assert_eq!(
        imports.iter().map(|import| import.symbol.as_str()).collect::<Vec<_>>(),
        vec!["panic_str"],
        "only the reachable authorized panic service may be emitted"
    );
}

#[test]
fn canonical_foundation_output_panic_call_has_the_authorized_direct_never_abi() {
    let (input, _isa, root) = canonical_foundation_output_fixture();
    let write = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Write"))
        .expect("canonical Core.Output Write source item");
    let call =
        find_corelib_service_call(input.database(), write, "__panic_str").expect("canonical Core.Output panic call");

    assert!(matches!(
        call_lowering(input.database(), call).expect("Core.Output panic lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__panic_str" && service.symbol == "panic_str"
    ));
    assert_eq!(
        beskid_codegen::SyntaxNodeFacts::new(&input).direct_callee(call),
        Some(DirectCallee::corelib_service("panic_str")),
        "the embedded Core.Output call must lower through the exact panic import"
    );
    assert_eq!(
        call_abi_signature(input.database(), call).expect("Core.Output panic ABI"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([beskid_queries::SemanticTypeId::STRING]),
            result: beskid_queries::SemanticTypeId::NEVER,
        })
    );
}

#[test]
fn canonical_foundation_error_panic_call_has_the_authorized_direct_never_abi() {
    let (input, _isa, root) = canonical_foundation_error_fixture();
    let write = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Write"))
        .expect("canonical Core.Error Write source item");
    let call =
        find_corelib_service_call(input.database(), write, "__panic_str").expect("canonical Core.Error panic call");

    assert!(matches!(
        call_lowering(input.database(), call).expect("Core.Error panic lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__panic_str" && service.symbol == "panic_str"
    ));
    assert_eq!(
        beskid_codegen::SyntaxNodeFacts::new(&input).direct_callee(call),
        Some(DirectCallee::corelib_service("panic_str")),
        "the embedded Core.Error call must lower through the exact panic import"
    );
    assert_eq!(
        call_abi_signature(input.database(), call).expect("Core.Error panic ABI"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([beskid_queries::SemanticTypeId::STRING]),
            result: beskid_queries::SemanticTypeId::NEVER,
        })
    );
}

#[test]
fn canonical_foundation_output_write_body_exposes_executable_block_statements() {
    let (input, _isa, root) = canonical_foundation_output_fixture();
    let write = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Write"))
        .expect("canonical Core.Output Write source item");
    let body = item_body(input.database(), write)
        .expect("canonical Core.Output body query")
        .expect("canonical Core.Output Write body");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    assert_eq!(
        facts.node_kind(body),
        Some(beskid_isle::NodeKind::BlockExpression),
        "an ordinary function body must use the shared executable-block lowering kind"
    );
    assert_eq!(facts.statement_count(body), Some(5), "the body cursor must enumerate unwrapped executable statements");
}
