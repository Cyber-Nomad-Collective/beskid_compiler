use std::collections::HashMap;
use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_corelib_syscall_service_capability,
    canonical_corelib_syscall_sources, canonical_runtime_intrinsic_capability,
    canonical_runtime_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
    SourceUnit, SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_codegen::{
    CodegenInput, ItemModuleImporter, emit_isle_expression, emit_isle_item,
    emit_isle_item_with_call_importer,
    module_emission::{SyntaxModuleItem, emit_syntax_program, lower_syntax_program},
};
use beskid_isle::{DirectCallee, FunctionEmitter, NodeFacts};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, CastIntent, Db, ProjectSession, SourceUnitId,
    SyntaxGenerationId, aggregate_field_access, build_canonical_corelib_syscall_typed_program,
    build_canonical_runtime_typed_program, build_typed_program,
    build_typed_program_with_corelib_services, call_abi_signature, call_lowering, child_nodes,
    closure_environment, enum_match, format_ast_node_site, item_body, item_name, literal_fact,
    mutable_local_assignment, node_kind, node_type, spawn_target, test_statement_nodes,
};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" fn test_system_allocate(size: usize, alignment: usize) -> *mut u8 {
    let Ok(layout) = std::alloc::Layout::from_size_align(size, alignment) else {
        return std::ptr::null_mut();
    };
    // The JIT regression reads the returned header and roots it immediately; intentionally keep
    // this test allocation alive until process exit because the canonical source owns no sweep.
    unsafe { std::alloc::alloc_zeroed(layout) }
}

#[test]
fn parsed_syntax_root_emits_verified_isle_clif_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 42; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let literal = find_integer_literal(&db, root).expect("integer literal key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_expression(&input, isa.as_ref(), literal, types::I32)
        .expect("parsed expression lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}

#[test]
fn parsed_multi_function_assembly_verification_error_identifies_the_originating_item_site() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Sibling() { return 1; } i32 Failing() { 2; }");
    let db = input.database();
    let items = find_function_definitions(db, root);
    let sibling = items[0];
    let failing = items[1];

    let error = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: sibling,
                symbol: "Sibling".into(),
            },
            SyntaxModuleItem {
                key: failing,
                symbol: "Failing".into(),
            },
        ],
    )
    .expect_err("the failing item must be rejected through module emission");
    let first = error.to_string();
    let repeated = error.to_string();
    let failing_site = format_ast_node_site(db, failing);
    let sibling_site = format_ast_node_site(db, sibling);

    assert_eq!(first, repeated);
    assert!(first.contains(&failing_site), "{first}");
    assert!(!first.contains(&sibling_site), "{first}");
    assert!(
        first.contains("syntax ISLE emission failed: Verification("),
        "{first}"
    );
    assert!(first.contains("FunctionDefinition@"), "{first}");
}

#[test]
fn parsed_statement_final_block_error_identifies_the_originating_body_site() {
    let (input, isa, root) = item_fixture_with_root("unit Main() { 2; }");
    let db = input.database();
    let body = find_node(
        db,
        root,
        beskid_queries::IndexedNodeKind::ExpressionStatement,
    )
    .expect("expression statement");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let emitter = FunctionEmitter::new(isa.as_ref());

    let error = emitter
        .emit_statement(
            UserFuncName::user(0, 100),
            emitter.signature([], [types::I32]),
            &facts,
            body,
        )
        .expect_err("non-unit fallthrough must fail final-block verification");
    let rendered = error.display_with_db(db);

    assert!(
        rendered.contains(&format_ast_node_site(db, body)),
        "{rendered}"
    );
    assert!(rendered.contains("ExpressionStatement@"), "{rendered}");
}

#[test]
fn parsed_parameter_materialization_error_identifies_the_originating_item_site() {
    let (input, isa, root) = item_fixture_with_root("i32 Main(i32 value) { return value; }");
    let db = input.database();
    let item = find_function_definition(db, root).expect("function item");
    let body = item_body(db, item)
        .expect("body query")
        .expect("function body");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let emitter = FunctionEmitter::new(isa.as_ref());

    let error = emitter
        .emit_item_statement(
            UserFuncName::user(0, 101),
            emitter.signature([], [types::I32]),
            &facts,
            item,
            body,
        )
        .expect_err("missing incoming parameter must fail materialization");
    let rendered = error.display_with_db(db);

    assert!(
        rendered.contains(&format_ast_node_site(db, item)),
        "{rendered}"
    );
    assert!(rendered.contains("FunctionDefinition@"), "{rendered}");
}

#[test]
fn unsupported_typed_operation_reports_deterministic_span_bearing_missing_rule() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Main(i32 outer) { let task = spawn ((i32 inner) => outer + inner); return outer; }",
    );
    let spawn = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::SpawnExpression,
    )
    .expect("spawn expression");

    let error = emit_isle_expression(&input, isa.as_ref(), spawn, types::I64)
        .expect_err("unsupported spawn must not route around generated ISLE");
    let first = error.display_with_db(input.database());
    let repeated = error.display_with_db(input.database());

    assert_eq!(first, repeated);
    assert!(first.contains("MissingRuleOrFact"), "{first}");
    assert!(first.contains("SpawnExpression@"), "{first}");
}

#[test]
fn unsupported_lambda_reports_deterministic_span_bearing_missing_rule() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Main(i32 outer) { let add = (i32 inner) => outer + inner; return outer; }",
    );
    let lambda = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::LambdaExpression,
    )
    .expect("lambda expression");

    assert_eq!(
        beskid_isle::classify_syntax_node_kind(
            beskid_queries::IndexedNodeKind::LambdaExpression
        ),
        beskid_isle::SyntaxNodeClassification::UnsupportedTypedOperation,
    );

    let error = emit_isle_expression(&input, isa.as_ref(), lambda, types::I64)
        .expect_err("unsupported lambda must not route around generated ISLE");
    let first = error.display_with_db(input.database());
    let repeated = error.display_with_db(input.database());

    assert_eq!(first, repeated);
    assert!(first.contains("MissingRuleOrFact"), "{first}");
    assert!(first.contains("LambdaExpression@"), "{first}");
}

#[test]
fn unsupported_code_string_reports_deterministic_span_bearing_missing_rule() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main() { code ```beskid\nlet generated = 1;\n```; return 0; }");
    let code_string = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::CodeStringLiteral,
    )
    .expect("code string literal");

    let error = emit_isle_expression(&input, isa.as_ref(), code_string, types::I64)
        .expect_err("code strings must not route around generated ISLE");
    let first = error.display_with_db(input.database());
    let repeated = error.display_with_db(input.database());

    assert_eq!(first, repeated);
    assert!(first.contains("MissingRuleOrFact"), "{first}");
    assert!(first.contains("CodeStringLiteral@"), "{first}");
}

#[test]
fn cast_facts_are_independent_of_the_shared_literal_syntax_classification() {
    let (input, _isa, root) = item_fixture_with_root("unit Main() { i64 widenedLiteral = 1; }");
    let literal = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::Literal,
    )
    .expect("typed literal");

    assert_eq!(
        beskid_isle::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::Literal),
        beskid_isle::SyntaxNodeClassification::IsleLowered(
            beskid_isle::NodeKind::LiteralExpression,
        )
    );
    assert_eq!(
        beskid_queries::cast_intents(input.database(), literal).expect("cast-intent query"),
        Some(Arc::from([CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::I64,
        }]))
    );
}

#[test]
fn closure_captures_and_spawn_target_are_independent_semantic_facts() {
    let (input, _isa, root) = item_fixture_with_root(
        "i32 Main(i32 outer) { let task = spawn ((i32 inner) => outer + inner); return outer; }",
    );
    let lambda = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::LambdaExpression,
    )
    .expect("lambda expression");
    let spawn = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::SpawnExpression,
    )
    .expect("spawn expression");
    let closure = closure_environment(input.database(), lambda)
        .expect("closure query")
        .expect("closure facts");
    let target = spawn_target(input.database(), spawn)
        .expect("spawn query")
        .expect("spawn facts");
    let function = find_function_definition(input.database(), root).expect("function definition");

    assert_eq!(
        beskid_isle::classify_syntax_node_kind(
            beskid_queries::IndexedNodeKind::LambdaExpression,
        ),
        beskid_isle::SyntaxNodeClassification::UnsupportedTypedOperation,
    );
    assert_eq!(
        beskid_isle::classify_syntax_node_kind(
            beskid_queries::IndexedNodeKind::SpawnExpression,
        ),
        beskid_isle::SyntaxNodeClassification::UnsupportedTypedOperation,
    );
    assert_eq!(closure.parameters.len(), 1, "lambda parameter fact");
    assert_eq!(closure.captures.len(), 1, "outer capture fact");
    assert_eq!(
        node_kind(input.database(), closure.captures[0].declaration)
            .expect("capture kind query"),
        Some(beskid_queries::IndexedNodeKind::Identifier),
    );
    assert_eq!(closure.captures[0].slot.owner, function);
    assert_eq!(closure.captures[0].slot.index, 0);
    assert_eq!(target.callee, lambda);
    assert_eq!(target.captures, closure.captures);
}

#[test]
fn parsed_struct_literal_uses_source_aggregate_layout_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source =
        "i32 Main() { let point = Point { x: 1, y: 2 }; return 0; } type Point { i32 x, i32 y }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let literal = find_node(
        &db,
        root,
        beskid_queries::IndexedNodeKind::StructLiteralExpression,
    )
    .expect("struct literal");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let function = emit_isle_expression(&input, isa.as_ref(), literal, isa.pointer_type())
        .expect("aggregate literal lowers through syntax facts");
    assert!(function.display().to_string().contains("stack_store"));
}

#[test]
fn parsed_enum_constructor_uses_source_layout_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "enum Choice { None(), Some(i32 value) } i32 Main() { Choice choice = Choice::Some(7); return 0; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let constructor = find_node(
        &db,
        root,
        beskid_queries::IndexedNodeKind::EnumConstructorExpression,
    )
    .expect("enum constructor");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("enum constructor lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("stack_store"));
    assert!(clif.contains("iconst.i32 1"));
}

#[test]
fn parsed_nullary_enum_constructor_uses_source_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { None(), Some(i32 value) } i32 Main() { Choice choice = Choice::None(); return 0; }",
    );
    let constructor = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::EnumConstructorExpression,
    )
    .expect("enum constructor");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("nullary enum constructor lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("stack_store"));
    assert!(clif.contains("iconst.i32 0"));
}

#[test]
fn parsed_enum_match_uses_source_arms_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { None(), Some() } i32 Main() { return match Choice::Some() { Choice::None() => 1, Choice::Some() => 2, }; }",
    );
    let expression = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::MatchExpression,
    )
    .expect("enum match");
    assert!(
        enum_match(input.database(), expression)
            .expect("enum match query")
            .is_some(),
        "source match facts"
    );
    assert_eq!(
        node_type(input.database(), expression).expect("match type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );
    let function = emit_isle_expression(&input, isa.as_ref(), expression, types::I32)
        .expect("enum match lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"));
    assert!(clif.contains("br_table"));
}

#[test]
fn parsed_function_body_emits_verified_isle_clif_without_lowerable() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 42; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed function body lowers through generated ISLE");

    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn parsed_u8_comparison_coerces_integer_literals_without_hir() {
    let (input, isa, item) = item_fixture("bool Main(u8 b) { return b > 57; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("u8 comparisons lower through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i8 57"), "{clif}");
}

#[test]
fn parsed_mixed_u8_i64_arithmetic_coerces_the_u8_operand_without_hir() {
    let (input, isa, item) = item_fixture("i64 Main(u8 b, i64 acc) { return acc + (b - 48); }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("mixed-width arithmetic lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(clif.contains("iadd"), "{clif}");
}

#[test]
fn parsed_nominal_parameter_field_read_lowers_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Style { i64 code } bool Main(Style chain) { return chain.code == 0; }",
    );
    let item = find_function_definition(input.database(), root).expect("main item");
    let field = find_node(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::PathExpression,
    )
    .expect("field expression");
    assert!(
        aggregate_field_access(input.database(), field)
            .expect("field query")
            .is_some(),
        "field access syntax fact"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("nominal parameter field read lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("load.i64"), "{clif}");
}

#[test]
fn parsed_test_item_emits_verified_isle_clif_without_lowerable() {
    let (input, isa, root) = item_fixture_with_root("test Smoke { return; }");
    let item = find_test_definition(input.database(), root).expect("test item key");

    let statements = test_statement_nodes(input.database(), item)
        .expect("test statement query")
        .expect("test statement nodes");
    assert_eq!(statements.len(), 1);
    assert_eq!(
        node_kind(input.database(), statements[0])
            .expect("statement kind")
            .expect("statement node"),
        beskid_queries::IndexedNodeKind::ReturnStatement
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed test item lowers through generated ISLE");

    assert!(function.display().to_string().contains("return"));
}

#[test]
fn parsed_local_read_emits_verified_isle_clif_without_lowerable() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { i32 answer = 42; return answer; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed local read lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}

#[test]
fn parsed_parameter_read_materializes_the_generation_safe_local_slot() {
    let (input, isa, item) = item_fixture("i32 Identity(i32 value) { return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed parameter read lowers through generated ISLE");
    let clif = function.display().to_string();
    assert!(clif.contains("function u0:0(i32) -> i32"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn parsed_mutable_range_accumulator_exposes_local_write_syntax_facts() {
    let (input, _isa, root) = item_fixture_with_root(
        "i32 Main() { mut i32 sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }",
    );
    let db = input.database();
    let assignment = find_node(db, root, beskid_queries::IndexedNodeKind::AssignExpression)
        .expect("parsed accumulator assignment");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    let target = facts.child(assignment, 0).expect("assignment target fact");
    let declaration = beskid_queries::resolved_local(db, target)
        .expect("assignment target resolution")
        .expect("assignment target local")
        .declaration;
    assert_eq!(
        mutable_local_assignment(db, assignment).expect("mutable assignment query"),
        Some(beskid_queries::MutableLocalAssignment {
            declaration,
            slot: beskid_queries::local_slot(db, declaration)
                .expect("assignment target slot")
                .expect("assignment target slot fact"),
        })
    );
    assert_eq!(facts.mutable_local_assignment_slot(assignment), Some(0));
}

#[test]
fn parsed_pointer_signature_uses_the_target_pointer_type_without_hir() {
    let (input, isa, item) = item_fixture("pointer Echo(pointer value) { return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("pointer syntax lowers through generated ISLE");
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
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    assert_eq!(
        beskid_queries::item_abi_signature(db, create).expect("generic item ABI query"),
        None,
        "generic factories must not expose a fixed item ABI"
    );
    let call = find_call_expression(db, main).expect("Create call");
    let signature = call_abi_signature(db, call)
        .expect("specialized call ABI query")
        .expect("Create<i64> specialization");
    assert_eq!(signature.result, beskid_queries::SemanticTypeId::POINTER);
    let _ = isa;
}

#[test]
fn parsed_direct_call_uses_explicit_item_module_importer() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let callee = items[0];
    let caller = items[1];
    let call = find_call_expression(db, caller).expect("call syntax key");
    let beskid_queries::CallLowering::Direct(declaration) = call_lowering(db, call)
        .expect("direct-call query")
        .expect("direct call")
    else {
        panic!("expected a syntax-resolved direct call");
    };
    assert_eq!(declaration, callee);

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I32, [types::I32]);
    let imported = module
        .declare_function("AddOne", Linkage::Import, &signature)
        .expect("declare imported syntax item");
    let mut importer = ItemModuleImporter::new(
        &mut module,
        HashMap::from([(beskid_isle::DirectCallee::item(declaration), imported)]),
    );

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
    let call = find_corelib_service_call(input.database(), read, "__syscall_read")
        .expect("__syscall_read call");
    let service = DirectCallee::corelib_service("syscall_read");
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), isa.pointer_type(), [types::I64, types::I64]);
    let imported = module
        .declare_function("syscall_read", Linkage::Import, &signature)
        .expect("declare the exact Corelib service import");
    let mut importer =
        ItemModuleImporter::new(&mut module, HashMap::from([(service.clone(), imported)]));
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.direct_callee(call), Some(service.clone()));
    assert_eq!(
        call_abi_signature(input.database(), call).expect("Corelib service ABI fact"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([
                beskid_queries::SemanticTypeId::I64,
                beskid_queries::SemanticTypeId::I64,
            ]),
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
    let mut importer =
        ItemModuleImporter::new(&mut module, HashMap::from([(service.clone(), imported)]));
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
fn canonical_foundation_assert_trigger_failure_lowers_only_the_panic_service() {
    let (input, isa, root) = canonical_foundation_assert_fixture();
    let trigger_failure = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| {
            item_name(input.database(), *key).ok().flatten().as_deref() == Some("trigger_failure")
        })
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
        &[SyntaxModuleItem {
            key: trigger_failure,
            symbol: "trigger_failure".into(),
        }],
    )
    .expect("canonical Assert lowers through syntax ISLE");
    let imports = &artifact.extern_imports;
    assert_eq!(
        imports
            .iter()
            .map(|import| import.symbol.as_str())
            .collect::<Vec<_>>(),
        vec!["panic_str"],
        "only the reachable authorized panic service may be emitted"
    );
}

#[test]
fn canonical_foundation_assert_public_helpers_lower_through_syntax_isle() {
    let (input, isa, root) = canonical_foundation_assert_fixture();
    // Non-generic helpers and their direct callees. Contains stays out: it pulls Core.String.
    // Equal is exercised below with an explicit call-derived i64 specialization.
    let items = [
        "trigger_failure",
        "Fail",
        "fail_with_because",
        "True",
        "False",
    ];
    let mut module_items = Vec::new();
    for name in items {
        let key = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("canonical Assert {name}"));
        module_items.push(SyntaxModuleItem {
            key,
            symbol: name.into(),
        });
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("canonical Assert helpers lower through syntax ISLE");
    for name in items {
        assert!(
            artifact.functions.iter().any(|function| function.name == name),
            "expected CLIF for {name}, got {:?}",
            artifact
                .functions
                .iter()
                .map(|function| function.name.as_str())
                .collect::<Vec<_>>()
        );
    }
    assert!(
        artifact
            .extern_imports
            .iter()
            .any(|import| import.symbol == "panic_str"),
        "Assert helpers must still import authorized panic_str"
    );
}

#[test]
fn canonical_foundation_assert_equal_specialization_lowers_through_syntax_isle() {
    let mut db = Box::new(BeskidDatabase::default());
    let assert_path =
        canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
            .expect("compiler-owned Assert path");
    let assert_source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source")
        .source;
    let directory = tempfile::tempdir().expect("project").keep();
    let main_path = directory.join("Main.bd");
    let main_source =
        "use Testing.Assert; unit Main() { Assert.Equal(1_i64, 1_i64, \"\"); }";
    std::fs::write(&main_path, main_source).expect("main source");
    // Prefer the compiler-owned Assert identity so panic_str authority remains available.
    let assert_program =
        parse_program_with_source_name(assert_path.to_str().unwrap(), &assert_source)
            .expect("assert parse");
    let main_program = parse_program_with_source_name(main_path.to_str().unwrap(), main_source)
        .expect("main parse");
    let main_unit = SourceUnitId::new(&*db, main_path.clone());
    let assert_unit = SourceUnitId::new(&*db, assert_path.clone());
    let generation = SyntaxGenerationId(97);
    let source_root = assert_path
        .ancestors()
        .nth(2)
        .expect("foundation src")
        .to_path_buf();
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        main_path.clone(),
        "beskid-foundation".into(),
        "assert-equal-specialization".into(),
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main".into(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
                path: assert_path,
                source: assert_source,
                program: assert_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("typed Assert+Main program");
    let main_root = AstNodeKey {
        unit: main_unit,
        generation,
        node: AstNodeId(0),
    };
    let assert_root = AstNodeKey {
        unit: assert_unit,
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from([main_root, assert_root]),
        target,
        manifest,
    )
    .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let mut module_items = Vec::new();
    for (root, name) in [
        (assert_root, "trigger_failure"),
        (assert_root, "Fail"),
        (assert_root, "fail_with_because"),
        (assert_root, "Equal"),
        (main_root, "Main"),
    ] {
        let key = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("expected {name}"));
        module_items.push(SyntaxModuleItem {
            key,
            symbol: name.into(),
        });
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("Assert.Equal specialization lowers through syntax ISLE");
    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Equal#generic_")),
        "expected specialized Equal CLIF, got {:?}",
        artifact
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn canonical_foundation_string_len_lowers_through_syntax_isle() {
    let mut db = Box::new(BeskidDatabase::default());
    let foundation_src = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("compiler-owned Assert path")
        .parent()
        .expect("Testing/")
        .parent()
        .expect("foundation src")
        .to_path_buf();
    let source_path = foundation_src.join("Core/String/String.bd");
    let source = std::fs::read_to_string(&source_path).expect("read Core.String");
    let source_root = foundation_src;
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source)
        .expect("parse Core.String");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "compiler-owned-foundation-string".into(),
    );
    let generation = SyntaxGenerationId(96);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Core/String/String.bd".into(),
            path: source_path,
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program(&mut db, project, generation, assembly)
        .expect("typed Core.String program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Core.String input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    // Leaf helpers that exercise dispatch builtins and string indexing without pulling the
    // full String.bd call graph (Contains -> IndexOfFrom -> while/ByteAt).
    for name in ["Len", "IsEmpty", "ByteAt"] {
        let key = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("Core.String {name}"));
        let module_items = if name == "IsEmpty" {
            let len = find_function_definitions(input.database(), root)
                .into_iter()
                .find(|key| {
                    item_name(input.database(), *key).ok().flatten().as_deref() == Some("Len")
                })
                .expect("Core.String Len");
            vec![
                SyntaxModuleItem {
                    key: len,
                    symbol: "Len".into(),
                },
                SyntaxModuleItem {
                    key,
                    symbol: name.into(),
                },
            ]
        } else {
            vec![SyntaxModuleItem {
                key,
                symbol: name.into(),
            }]
        };
        lower_syntax_program(&input, isa.as_ref(), &module_items).unwrap_or_else(|error| {
            panic!("Core.String {name} lowers through syntax ISLE: {error:?}")
        });
    }
}

#[test]
fn copied_foundation_assert_source_cannot_receive_panic_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let directory = tempfile::tempdir()
        .expect("copied Foundation project")
        .keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("Assert parent"))
        .expect("create copied Assert parent");
    std::fs::write(&source_path, &source.source).expect("write copied Assert source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse copied Foundation Assert source");
    let generation = SyntaxGenerationId(95);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory.clone(),
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let project = ProjectSession::new(
        &db,
        directory,
        source_path.clone(),
        "copied-foundation".into(),
        "copied-assert".into(),
    );
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("copied source remains an ordinary syntax program");
    assert!(typed.corelib_service_capability.is_none());

    let trigger_failure = SyntaxIndex::from_program(&program, generation)
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey {
            unit: SourceUnitId::new(&db, source_path.clone()),
            generation,
            node,
        })
        .find(|key| {
            call_lowering(&db, *key)
                .ok()
                .flatten()
                .is_some_and(|lowering| matches!(lowering, beskid_queries::CallLowering::Dynamic))
        })
        .expect("copied panic spelling remains dynamic");
    assert!(matches!(
        call_lowering(&db, trigger_failure).expect("copied call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_foundation_assert_source_cannot_receive_panic_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let directory = tempfile::tempdir()
        .expect("symlinked Foundation project")
        .keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("Assert parent"))
        .expect("create symlinked Assert parent");
    let compiler_owned_path =
        canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
            .expect("compiler-owned Assert path");
    std::os::unix::fs::symlink(&compiler_owned_path, &source_path)
        .expect("link compiler-owned Assert source into user project");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse symlinked Foundation Assert source");
    let generation = SyntaxGenerationId(96);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory.clone(),
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let project = ProjectSession::new(
        &db,
        directory,
        source_path.clone(),
        "symlinked-foundation".into(),
        "symlinked-assert".into(),
    );
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("symlinked source remains an ordinary syntax program");
    assert!(typed.corelib_service_capability.is_none());

    let trigger_failure = SyntaxIndex::from_program(&program, generation)
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey {
            unit: SourceUnitId::new(&db, source_path.clone()),
            generation,
            node,
        })
        .find(|key| {
            call_lowering(&db, *key)
                .ok()
                .flatten()
                .is_some_and(|lowering| matches!(lowering, beskid_queries::CallLowering::Dynamic))
        })
        .expect("symlinked panic spelling remains dynamic");
    assert!(matches!(
        call_lowering(&db, trigger_failure).expect("symlinked call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    ));
}

#[test]
fn ordinary_syscall_spelling_cannot_request_a_corelib_service_import() {
    let (input, _isa, root) =
        item_fixture_with_root("i64 Main() { return __syscall_write(1, \"application\"); }");
    let main = find_function_definition(input.database(), root).expect("application Main");
    let call = find_call_expression(input.database(), main).expect("application syscall spelling");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.direct_callee(call), None);
}

#[test]
fn parsed_program_declares_then_imports_syntax_items_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "AddOne".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
        Linkage::Export,
    )
    .expect("syntax items declare before their direct-call bodies lower");
    assert_eq!(declared.len(), 2);
    assert_eq!(
        module.get_name("AddOne"),
        Some(cranelift_module::FuncOrDataId::Func(
            declared[&beskid_isle::DirectCallee::item(items[0])]
        ))
    );
    assert_eq!(
        module.get_name("Main"),
        Some(cranelift_module::FuncOrDataId::Func(
            declared[&beskid_isle::DirectCallee::item(items[1])]
        ))
    );
}

#[test]
fn parsed_program_lowers_to_backend_artifact_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "AddOne".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax items lower into a normal backend artifact");

    assert_eq!(artifact.functions.len(), 2);
    beskid_codegen::validate_artifact(&artifact)
        .expect("direct syntax calls resolve against artifact definitions");
    let main = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_syntax_program_omits_uncalled_generic_enum_declarations() {
    let (input, isa, root) = item_fixture_with_root(
        "type Box<T> { T value } enum Option<T> { Some(T value), None } i32 Main() { return 0; }",
    );
    let boxed = find_definition_of_kind(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::TypeDefinition,
    )
    .expect("generic type declaration");
    let option = find_definition_of_kind(
        input.database(),
        root,
        beskid_queries::IndexedNodeKind::EnumDefinition,
    )
    .expect("generic enum declaration");
    let main = find_function_definitions(input.database(), root)[0];

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: boxed,
                symbol: "Box".into(),
            },
            SyntaxModuleItem {
                key: option,
                symbol: "Option".into(),
            },
            SyntaxModuleItem {
                key: main,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("generic declarations without executable bodies are omitted");

    assert_eq!(
        artifact
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        ["Main"],
        "only executable syntax items enter the artifact",
    );
}

#[test]
fn parsed_struct_literal_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main() { return Point { x: 1 }.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("inline method source item");
    assert_eq!(
        beskid_isle::classify_syntax_node_kind(
            beskid_queries::IndexedNodeKind::MethodDefinition
        ),
        beskid_isle::SyntaxNodeClassification::IsleLowered(
            beskid_isle::NodeKind::MethodDefinition
        ),
        "MethodDefinition must be production-supported at the ISLE inventory boundary"
    );
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(
        facts.node_kind(method),
        Some(beskid_isle::NodeKind::MethodDefinition),
        "adapter must surface MethodDefinition as an IsleLowered item kind"
    );
    let call = find_call_expression(db, main).expect("method call syntax");
    let beskid_queries::CallLowering::Direct(declaration) = call_lowering(db, call)
        .expect("method call query")
        .expect("method call lowering")
    else {
        panic!("struct literal method call must resolve to its exact syntax declaration");
    };
    assert_eq!(declaration, method);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: method,
                symbol: "Point_Ping".into(),
            },
            SyntaxModuleItem {
                key: main,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax-only module lowering supports the method receiver ABI");

    beskid_codegen::validate_artifact(&artifact)
        .expect("method call imports the exact syntax method declaration");
    let main = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_nominal_parameter_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main(Point point) { return point.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("inline method source item");
    let call = find_call_expression(db, main).expect("method call syntax");
    assert_eq!(
        call_lowering(db, call).expect("method call query"),
        Some(beskid_queries::CallLowering::Direct(method))
    );

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: method,
                symbol: "Point_Ping".into(),
            },
            SyntaxModuleItem {
                key: main,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax-only module lowering supports an explicit nominal receiver ABI");

    beskid_codegen::validate_artifact(&artifact)
        .expect("nominal receiver call imports its exact syntax method declaration");
    let main = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_program_specializes_an_inferred_generic_call_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } unit Main() { Equal(\"same\", \"same\", \"because\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "Equal".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax module specializes inferred generic calls through exact ABI facts");

    beskid_codegen::validate_artifact(&artifact)
        .expect("the generic call imports its specialized item identity");
    assert_eq!(artifact.functions.len(), 2);
    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Equal#generic_")),
        "generic source items must use a mangled specialization identity"
    );
}

#[test]
fn parsed_program_specializes_zero_argument_generic_factory_without_hir() {
    // Channel<T> Create<T>() collapses to POINTER at the ABI layer. Item ABI must still refuse a
    // fixed signature so module emission registers SpecializedItem, matching call-site imports.
    let (input, isa, root) = item_fixture_with_root(
        "type Channel<T> { i64 handle } Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; } unit Main() { Channel<i64> ch = Create<i64>(); return; }",
    );
    let items = find_function_definitions(input.database(), root);
    let create = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Create"))
        .expect("Create");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    assert_eq!(
        beskid_queries::item_abi_signature(input.database(), create).expect("generic item ABI"),
        None
    );

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: create,
                symbol: "Create".into(),
            },
            SyntaxModuleItem {
                key: main,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("zero-argument generic factories specialize through call-derived ABI identity");

    beskid_codegen::validate_artifact(&artifact)
        .expect("specialized factory imports must resolve against module declarations");
    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Create#generic_")),
        "generic factory must emit a mangled specialization, not a bare Item identity"
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
            SyntaxModuleItem {
                key: generic,
                symbol: "Equal".into(),
            },
            SyntaxModuleItem {
                key: test,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("test-body generic calls produce exact syntax ABI specializations");

    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Equal#generic_")),
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
            SyntaxModuleItem {
                key: items[0],
                symbol: "Position".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Equal".into(),
            },
            SyntaxModuleItem {
                key: test,
                symbol: "Main".into(),
            },
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
fn parsed_program_specializes_a_qualified_imported_generic_call_without_hir() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("project").keep();
    let main_path = directory.join("Main.bd");
    let assert_path = directory.join("Testing/Assert.bd");
    let main_source =
        "use Testing.Assert; test Main { Assert.Equal(\"same\", \"same\", \"because\"); }";
    let assert_source = "pub unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; }";
    std::fs::create_dir_all(assert_path.parent().expect("Testing directory"))
        .expect("Testing directory");
    std::fs::write(&main_path, main_source).expect("main source");
    std::fs::write(&assert_path, assert_source).expect("assert source");
    let main_program = parse_program_with_source_name(main_path.to_str().unwrap(), main_source)
        .expect("main parse");
    let assert_program =
        parse_program_with_source_name(assert_path.to_str().unwrap(), assert_source)
            .expect("assert parse");
    let main_unit = SourceUnitId::new(&*db, main_path.clone());
    let assert_unit = SourceUnitId::new(&*db, assert_path.clone());
    let generation = SyntaxGenerationId(22);
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        main_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main".into(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: "Testing.Assert".into(),
                path: assert_path,
                source: assert_source.into(),
                program: assert_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_root = AstNodeKey {
        unit: main_unit,
        generation,
        node: AstNodeId(0),
    };
    let assert_root = AstNodeKey {
        unit: assert_unit,
        generation,
        node: AstNodeId(0),
    };
    let generic = find_function_definition(&*db, assert_root).expect("generic function");
    let test = find_test_definition(&*db, main_root).expect("test item");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from([main_root, assert_root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: generic,
                symbol: "Equal".into(),
            },
            SyntaxModuleItem {
                key: test,
                symbol: "Main".into(),
            },
        ],
    )
    .expect("qualified generic calls produce exact syntax ABI specializations");

    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Equal#generic_")),
        "qualified generic calls must emit their exact specialization",
    );
}

#[test]
fn parsed_syntax_program_uses_the_existing_artifact_string_pool() {
    let (input, isa, root) = item_fixture_with_root("unit Main() { \"Beskid\"; return; }");
    let main = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem {
            key: main,
            symbol: "Main".into(),
        }],
    )
    .expect("syntax item with a string literal lowers through the artifact pool");

    assert_eq!(artifact.string_literals.len(), 1);
    assert!(
        artifact
            .string_literals
            .values()
            .any(|bytes| bytes.as_slice() == b"Beskid")
    );
}

#[test]
fn parsed_syntax_program_emits_imported_unit_calls_as_statements() {
    let (input, isa, root) = item_fixture_with_root("unit Assert() { } unit Main() { Assert(); }");
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "Assert".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax program with a unit call lowers through its statement rule");

    let main = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("Main function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn canonical_runtime_allocation_and_root_frame_helpers_emit_verified_clif_with_manifest_imports() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let source = canonical_runtime_sources()
        .pop()
        .expect("embedded canonical runtime source");
    let source_path = directory.join("Bootstrap.bd");
    std::fs::write(&source_path, &source.source).expect("write canonical runtime source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse canonical runtime source");
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(31);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("canonical runtime syntax facts");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&*db, source_path),
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("canonical runtime codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let items = find_function_definitions(input.database(), root);
    let selected = [
        "NativePointer",
        "SystemAllocate",
        "RootFramePrevious",
        "RootFrame",
    ];
    let module_items = selected
        .into_iter()
        .map(|name| {
            let key = items
                .iter()
                .copied()
                .find(|key| {
                    item_name(input.database(), *key).ok().flatten().as_deref() == Some(name)
                })
                .unwrap_or_else(|| panic!("canonical helper {name}"));
            SyntaxModuleItem {
                key,
                symbol: name.into(),
            }
        })
        .collect::<Vec<_>>();

    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("canonical helpers lower through the syntax-only module emitter");

    beskid_codegen::validate_artifact(&artifact)
        .expect("canonical helper imports are declared by the manifest authority");
    let imports = beskid_codegen::referenced_extern_imports(&artifact);
    assert!(
        imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_system_allocate")
    );
    let root_frame = artifact
        .functions
        .iter()
        .find(|function| function.name == "RootFrame")
        .expect("RootFrame helper is lowered");
    assert!(
        root_frame
            .function
            .display()
            .to_string()
            .contains("load.i64"),
        "manifest-authorized raw_word_load is lowered inline through ISLE"
    );
    assert!(
        !imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_raw_word_load"),
        "the inline load must not retain an unnecessary ABI import"
    );
    assert!(
        root_frame.function.display().to_string().contains("iadd"),
        "manifest-authorized pointer_add is lowered inline through ISLE"
    );
    assert!(
        !imports.iter().any(|entry| {
            matches!(
                entry.symbol.as_str(),
                "beskid_rt_v5_intrinsic_pointer_from_native_word"
                    | "beskid_rt_v5_intrinsic_pointer_add"
            )
        }),
        "inline pointer conversions and arithmetic must not retain ABI imports"
    );
    assert_eq!(
        imports
            .iter()
            .map(|entry| entry.symbol.as_str())
            .collect::<Vec<_>>(),
        ["beskid_rt_v5_intrinsic_system_allocate"],
        "only the still-external allocation primitive remains imported"
    );

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &module_items,
        Linkage::Export,
    )
    .expect("canonical runtime helpers define through the production module emitter");
    assert_eq!(declared.len(), module_items.len());
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn canonical_runtime_closure_descriptor_validation_and_rooting_execute_fail_closed() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let source = canonical_runtime_sources()
        .pop()
        .expect("embedded canonical runtime source");
    let source_path = directory.join("Bootstrap.bd");
    std::fs::write(&source_path, &source.source).expect("write canonical runtime source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse canonical runtime source");
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(32);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("canonical runtime syntax facts");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&*db, source_path),
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("canonical runtime codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let items = find_function_definitions(input.database(), root);
    let selected = [
        "NativePointer",
        "NativeWord",
        "NativeWordMax",
        "SystemAllocate",
        "AllocationSize",
        "AllocationAlignment",
        "AllocationDescriptor",
        "InitializeObjectHeader",
        "TypeDescriptorSize",
        "TypeDescriptorAlignment",
        "TypeDescriptorPointerMap",
        "TypeDescriptorPointerCount",
        "IsValidObjectAlignment",
        "ValidatePointerMap",
        "ValidateTypeDescriptor",
        "AllocateClosureEnvironment",
        "RootFrame",
        "RootFrameSlots",
        "RootFrameSlotCount",
        "SetRootSlotValue",
        "RootClosureEnvironment",
    ];
    let module_items = selected
        .into_iter()
        .map(|name| {
            let key = items
                .iter()
                .copied()
                .find(|key| {
                    item_name(input.database(), *key).ok().flatten().as_deref() == Some(name)
                })
                .unwrap_or_else(|| panic!("canonical helper {name}"));
            SyntaxModuleItem {
                key,
                symbol: name.into(),
            }
        })
        .collect::<Vec<_>>();
    let mut builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
    builder.symbol(
        "beskid_rt_v5_intrinsic_system_allocate",
        test_system_allocate as *const u8,
    );
    let mut module = JITModule::new(builder);
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &module_items,
        Linkage::Export,
    )
    .expect("closure descriptor helpers lower through the production module emitter");
    module.finalize_definitions().expect("finalize closure helpers");

    let validate = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref()
                            == Some("ValidateTypeDescriptor")
                    })
                    .expect("ValidateTypeDescriptor item"),
            ))
            .expect("ValidateTypeDescriptor declaration"),
    );
    let root_environment = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref()
                            == Some("RootClosureEnvironment")
                    })
                    .expect("RootClosureEnvironment item"),
            ))
            .expect("RootClosureEnvironment declaration"),
    );
    let allocate_environment = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref()
                            == Some("AllocateClosureEnvironment")
                    })
                    .expect("AllocateClosureEnvironment item"),
            ))
            .expect("AllocateClosureEnvironment declaration"),
    );
    let validate: extern "C" fn(*const usize) -> u8 = unsafe { std::mem::transmute(validate) };
    let root_environment: extern "C" fn(*mut usize, usize, *mut u8) -> u8 =
        unsafe { std::mem::transmute(root_environment) };
    let allocate_environment: extern "C" fn(*const usize) -> *mut u8 =
        unsafe { std::mem::transmute(allocate_environment) };

    let mut pointer_map = [16usize];
    let mut descriptor = [32usize, 8, pointer_map.as_mut_ptr() as usize, 1, 0];
    assert_eq!(validate(descriptor.as_ptr()), 1, "valid descriptor is accepted");

    pointer_map[0] = 17;
    assert_eq!(validate(descriptor.as_ptr()), 0, "unaligned pointer offset is rejected");
    pointer_map[0] = usize::MAX;
    assert_eq!(validate(descriptor.as_ptr()), 0, "overflowing pointer end is rejected");
    pointer_map[0] = 16;
    descriptor[1] = 24;
    assert_eq!(validate(descriptor.as_ptr()), 0, "non-power-of-two alignment is rejected");
    assert_eq!(validate(std::ptr::null()), 0, "null descriptor is rejected before dereference");

    descriptor[1] = 8;
    let request = [32usize, 8, descriptor.as_mut_ptr() as usize];
    let environment = allocate_environment(request.as_ptr());
    assert!(!environment.is_null(), "valid request allocates a closure environment");
    let header = environment as *const usize;
    assert_eq!(unsafe { *header }, descriptor.as_mut_ptr() as usize);
    assert_eq!(unsafe { *header.add(1) }, 0, "allocation clears the GC word");

    let mut slots = [0usize];
    let mut frame = [0usize, slots.as_mut_ptr() as usize, 1];
    let mut tls = [0usize, frame.as_mut_ptr() as usize, 0, 1];
    assert_eq!(root_environment(tls.as_mut_ptr(), 0, environment), 1);
    assert_eq!(slots[0], environment as usize, "valid environment is rooted in its slot");
}

fn item_fixture(
    source: &str,
) -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let (input, isa, root) = item_fixture_with_root(source);
    let item = find_function_definition(input.database(), root).expect("function key");
    (input, isa, item)
}

fn canonical_corelib_syscall_fixture() -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("Corelib syscall project").keep();
    let source = canonical_corelib_syscall_sources()
        .pop()
        .expect("embedded Core.Syscall source");
    let source_path = directory.join("Syscall.bd");
    std::fs::write(&source_path, &source.source).expect("write embedded Core.Syscall source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse embedded Core.Syscall source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "corelib-source".into(),
    );
    let generation = SyntaxGenerationId(92);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            path: source_path,
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_corelib_syscall_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_syscall_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("exact embedded Core.Syscall source receives service authority");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Corelib input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn materialized_corelib_syscall_fixture() -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir()
        .expect("materialized Corelib syscall project")
        .keep();
    let source = canonical_corelib_syscall_sources()
        .pop()
        .expect("embedded Core.Syscall source");
    let source_path = directory.join("obj/beskid/deps/src/foundation/Core/Syscall/Syscall.bd");
    std::fs::create_dir_all(source_path.parent().expect("materialized syscall parent"))
        .expect("create materialized syscall parent");
    std::fs::write(&source_path, &source.source).expect("write materialized Core.Syscall source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse materialized Core.Syscall source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "materialized-corelib-source".into(),
    );
    let generation = SyntaxGenerationId(97);
    let assembly = ProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory.clone(),
            },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: directory.join("obj/beskid/deps/src/foundation"),
            }],
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: source_path.display().to_string(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        hir_units: Arc::new(Vec::new()),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths: Arc::from([source_path.clone()]),
    };
    let syntax = Arc::new(SyntaxProgramAssembly::from(&assembly));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        syntax,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("loader-proven materialized Core.Syscall receives service authority");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe materialized Corelib input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn canonical_foundation_assert_fixture() -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let mut db = Box::new(BeskidDatabase::default());
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let source_path =
        canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
            .expect("compiler-owned Assert path");
    let source_root = source_path
        .ancestors()
        .nth(2)
        .expect("foundation source root")
        .to_path_buf();
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse embedded Foundation Assert source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "compiler-owned-foundation".into(),
    );
    let generation = SyntaxGenerationId(94);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path,
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("compiler-owned Assert source receives service authority");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Foundation Assert input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn item_fixture_with_root(
    source: &str,
) -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(21);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn function_signature(
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    result: cranelift_codegen::ir::Type,
    parameters: impl IntoIterator<Item = cranelift_codegen::ir::Type>,
) -> cranelift_codegen::ir::Signature {
    let mut signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    signature.params.extend(
        parameters
            .into_iter()
            .map(cranelift_codegen::ir::AbiParam::new),
    );
    signature
        .returns
        .push(cranelift_codegen::ir::AbiParam::new(result));
    signature
}

fn find_function_definitions(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Vec<AstNodeKey> {
    let mut found = Vec::new();
    if node_kind(db, key).ok().flatten()
        == Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        found.push(key);
    }
    if let Some(children) = child_nodes(db, key).ok().flatten() {
        for child in children.iter().copied() {
            found.extend(find_function_definitions(db, child));
        }
    }
    found
}

fn find_definition_of_kind(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    expected: beskid_queries::IndexedNodeKind,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(expected) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_definition_of_kind(db, child, expected))
}

fn find_call_expression(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::CallExpression) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_call_expression(db, child))
}

fn find_corelib_service_call(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    expected_name: &str,
) -> Option<AstNodeKey> {
    if matches!(
        call_lowering(db, key).ok().flatten(),
        Some(beskid_queries::CallLowering::CorelibService(service)) if service.name == expected_name
    ) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_corelib_service_call(db, child, expected_name))
}

struct CorelibServiceImportFacts {
    call: AstNodeKey,
    fd: AstNodeKey,
    limit: AstNodeKey,
    service: DirectCallee,
}

impl CorelibServiceImportFacts {
    fn new(db: &dyn beskid_queries::Db, service: DirectCallee) -> Self {
        let unit = SourceUnitId::new(db, std::path::PathBuf::from("/tmp/CorelibService.bd"));
        let generation = SyntaxGenerationId(93);
        Self {
            call: AstNodeKey {
                unit,
                generation,
                node: AstNodeId(1),
            },
            fd: AstNodeKey {
                unit,
                generation,
                node: AstNodeId(2),
            },
            limit: AstNodeKey {
                unit,
                generation,
                node: AstNodeId(3),
            },
            service,
        }
    }
}

impl NodeFacts for CorelibServiceImportFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<beskid_isle::NodeKind> {
        (key == self.call)
            .then_some(beskid_isle::NodeKind::CallExpression)
            .or_else(|| {
                (key == self.fd || key == self.limit)
                    .then_some(beskid_isle::NodeKind::LiteralExpression)
            })
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<beskid_isle::LiteralKind> {
        (key == self.fd || key == self.limit).then_some(beskid_isle::LiteralKind::Integer)
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<beskid_isle::CallKind> {
        (key == self.call).then_some(beskid_isle::CallKind::Direct)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.fd)
            .then_some(0)
            .or_else(|| (key == self.limit).then_some(16))
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.call {
            Some(types::I64)
        } else {
            (key == self.fd || key == self.limit).then_some(types::I64)
        }
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        (key == self.call).then_some(self.service.clone())
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Signature> {
        (key == self.call).then(|| cranelift_codegen::ir::Signature {
            params: vec![
                cranelift_codegen::ir::AbiParam::new(types::I64),
                cranelift_codegen::ir::AbiParam::new(types::I64),
            ],
            returns: vec![cranelift_codegen::ir::AbiParam::new(types::I64)],
            call_conv: cranelift_codegen::isa::CallConv::SystemV,
        })
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.call).then_some(vec![self.fd, self.limit])
    }
}

fn find_function_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key)
        .ok()
        .flatten()
        .is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_function_definition(db, child))
}

fn find_test_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key)
        .ok()
        .flatten()
        .is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::TestDefinition)
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_test_definition(db, child))
}

fn find_integer_literal(db: &BeskidDatabase, key: AstNodeKey) -> Option<AstNodeKey> {
    if literal_fact(db, key)
        .ok()
        .flatten()
        .is_some_and(|fact| matches!(fact, beskid_queries::LiteralFact::Integer(value) if value.as_ref() == "42"))
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_integer_literal(db, child))
}

fn find_node(
    db: &dyn Db,
    key: AstNodeKey,
    expected: beskid_queries::IndexedNodeKind,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(expected) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .find_map(|child| find_node(db, *child, expected))
}
