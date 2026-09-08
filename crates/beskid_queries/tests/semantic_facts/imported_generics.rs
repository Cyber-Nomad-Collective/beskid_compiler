use super::support::{assert_unavailable, key};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AggregateFieldShape, AstNodeKey, BeskidDatabase, EnumLayoutTemplateArgument, GenericSubstitution, ItemSignature,
    ProjectSession, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type, build_typed_program,
    call_abi_signature, call_arguments, call_lowering, enum_constructor_specialization, enum_constructor_template,
    enum_match, generic_call_instantiation, generic_call_specialization, item_abi_signature, node_type, value_abi_type,
};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn generic_imported_static_call_resolves_to_its_exact_syntax_item() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-import/project/src");
    let main_path = root.join("Main.bd");
    let channel_path = root.join("Concurrency/Channel.bd");
    let main_source = "use Concurrency.Channel;\nunit Main() { Channel<i64>.Create(); }";
    let channel_source = "pub unit Create<T>() { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let channel_program =
        expand_program(parse_program(channel_source).expect("channel parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(19);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: channel_path.display().to_string(),
                path: channel_path.clone(),
                source: channel_source.to_string(),
                program: channel_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let channel_unit = SourceUnitId::new(&db, channel_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let channel_index = SyntaxIndex::from_program(&channel_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(channel_unit, generation, &channel_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("generic imported static call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn imported_generic_type_annotation_resolves_without_registry_reentrance() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-type/project/src");
    let main_path = root.join("Main.bd");
    let envelope_path = root.join("Messaging/Envelope.bd");
    let main_source =
        "use Messaging.Envelope;\nunit Main() { Envelope<i64> envelope = Envelope<i64> { value: 1 }; return; }";
    let envelope_source = "pub type Envelope<T> { i64 value }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let envelope_program =
        expand_program(parse_program(envelope_source).expect("envelope parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(20);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: envelope_path.display().to_string(),
                path: envelope_path.clone(),
                source: envelope_source.to_string(),
                program: envelope_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let local = key(main_unit, generation, &main_index, NodeKind::LetStatement, 0);

    assert_eq!(abi_type(&db, local).expect("imported generic type annotation ABI"), Some(SemanticTypeId::POINTER));
}

#[test]
fn imported_generic_nominal_calls_require_receiver_instantiation() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-nominal-receiver/project/src");
    let main_path = root.join("Main.bd");
    let hub_path = root.join("Collections/Hub.bd");
    let main_source =
        "use Collections.Hub;\nunit Main() { Hub.Create(); Hub<i64>.Create(1_i64); Hub.Create<i64>(1_i64); }";
    let hub_source = "type Hub<T> { i64 value }\npub unit Create<T>(T value) { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let hub_program = expand_program(parse_program(hub_source).expect("hub parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(55);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: hub_path.display().to_string(),
                path: hub_path.clone(),
                source: hub_source.to_string(),
                program: hub_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let hub_unit = SourceUnitId::new(&db, hub_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let hub_index = SyntaxIndex::from_program(&hub_program, generation);
    let declaration = key(hub_unit, generation, &hub_index, NodeKind::FunctionDefinition, 0);
    let missing_receiver = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let explicit_receiver = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);
    let method_generic = key(main_unit, generation, &main_index, NodeKind::CallExpression, 2);

    assert_unavailable(call_lowering(&db, missing_receiver));
    assert_unavailable(call_abi_signature(&db, missing_receiver));
    // An unavailable call site yields no call-derived ABI specialization. The query returns
    // `Ok(None)` (no specialization) rather than propagating the unavailable error, so reachable
    // Syscall/Output bodies with unresolved calls do not abort whole-module emission.
    assert_eq!(
        generic_call_specialization(&db, missing_receiver)
            .expect("missing receiver yields no specialization rather than an error"),
        None
    );
    assert_eq!(
        call_lowering(&db, explicit_receiver).expect("explicit receiver lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, explicit_receiver).expect("explicit receiver instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    assert_eq!(
        call_lowering(&db, method_generic).expect("method generic lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, method_generic).expect("method generic instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
}

#[test]
fn generic_imported_terminal_call_requires_an_exact_declared_generic_arity() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-terminal-import/project/src");
    let main_path = root.join("Main.bd");
    let channel_path = root.join("Concurrency/Channel.bd");
    let main_source = "use Concurrency.Channel;\nunit Main() { Channel.CreateWithOptions<i64>(); Channel.CreateWithOptions<i64, i32>(); }";
    let channel_source = "pub unit CreateWithOptions<T>() { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let channel_program =
        expand_program(parse_program(channel_source).expect("channel parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(20);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: channel_path.display().to_string(),
                path: channel_path.clone(),
                source: channel_source.to_string(),
                program: channel_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let channel_unit = SourceUnitId::new(&db, channel_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let channel_index = SyntaxIndex::from_program(&channel_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(channel_unit, generation, &channel_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("generic imported terminal call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, call).expect("exact generic instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    let mismatched = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);
    assert_eq!(
        call_lowering(&db, mismatched).expect("mismatched generic terminal call"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
    assert_eq!(generic_call_instantiation(&db, mismatched).expect("mismatched generic instantiation"), None);
}

#[test]
fn generic_imported_receiver_call_uses_receiver_specialization_for_zero_argument_abi() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-receiver-import/project/src");
    let main_path = root.join("Main.bd");
    let hub_path = root.join("System/Hub.bd");
    let main_source = r#"
use System.Hub;
unit Main() {
    Hub<i64>.Create();
    Hub.Create();
}

"#;
    let hub_source = r#"
type Hub<T> { i64 value }
pub Hub<T> Create<T>() { return Hub<T> { value: 0_i64 }; }
"#;
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let hub_program = expand_program(parse_program(hub_source).expect("hub parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(22);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: hub_path.display().to_string(),
                path: hub_path.clone(),
                source: hub_source.to_string(),
                program: hub_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let hub_unit = SourceUnitId::new(&db, hub_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let hub_index = SyntaxIndex::from_program(&hub_program, generation);
    let declaration = key(hub_unit, generation, &hub_index, NodeKind::FunctionDefinition, 0);
    let receiver_specialized = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let bare = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);

    assert_eq!(
        call_lowering(&db, receiver_specialized).expect("receiver specialization"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, receiver_specialized).expect("receiver specialization fact"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    assert_eq!(
        call_abi_signature(&db, receiver_specialized).expect("receiver call ABI"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        beskid_queries::abi_type(&db, receiver_specialized).expect("receiver call result ABI"),
        Some(SemanticTypeId::POINTER)
    );
    assert_unavailable(call_lowering(&db, bare));
}

#[test]
fn imported_homonymous_module_generic_envelope_retains_pointer_abi_specialization() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-envelope/project/src");
    let main_path = root.join("Main.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let error_path = root.join("Core/Syscall/SyscallError.bd");
    let syscall_path = root.join("Core/Syscall/Syscall.bd");
    let main_source = r#"
use Core.Results;
use Core.Syscall;
unit Main() {
    Core.Results.Result<i64, Core.Syscall.SyscallError> result = Core.Syscall.Write();
    Results.IsOk(result);
}
"#;
    let results_source = r#"
pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
pub bool IsOk<TValue, TError>(Result<TValue, TError> value) { return true; }
"#;
    let error_source = "pub enum SyscallError { InvalidFd(i64 fd) }";
    let syscall_source = r#"
use Core.Syscall.SyscallError;
pub Core.Results.Result<i64, Core.Syscall.SyscallError> Write() {
    return Result::Error(SyscallError::InvalidFd(1_i64));
}
"#;
    let sources = [
        (&main_path, main_source),
        (&results_path, results_source),
        (&error_path, error_source),
        (&syscall_path, syscall_source),
    ];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let main_program = units[0].program.clone();
    let results_program = units[1].program.clone();
    let syscall_program = units[3].program.clone();
    let generation = SyntaxGenerationId(23);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let results_unit = SourceUnitId::new(&db, results_path);
    let syscall_unit = SourceUnitId::new(&db, syscall_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let results_index = SyntaxIndex::from_program(&results_program, generation);
    let syscall_index = SyntaxIndex::from_program(&syscall_program, generation);
    let declaration = key(results_unit, generation, &results_index, NodeKind::FunctionDefinition, 0);
    let write = key(syscall_unit, generation, &syscall_index, NodeKind::FunctionDefinition, 0);
    let call = main_index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: main_unit, generation, node })
        .find(|call| {
            generic_call_specialization(&db, *call)
                .ok()
                .flatten()
                .is_some_and(|specialization| specialization.declaration == declaration)
        })
        .expect("Results.IsOk call");

    assert_eq!(
        item_abi_signature(&db, write).expect("Syscall.Write item ABI"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        call_lowering(&db, call).expect("qualified Results.IsOk lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        call_abi_signature(&db, call).expect("generic Results.IsOk call ABI"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL })
    );
    assert_eq!(
        generic_call_specialization(&db, call).expect("generic Results.IsOk specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration,
            signature: ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL },
            substitutions: Arc::from([]),
        })
    );
}

#[test]
fn item_abi_signature_resolves_exact_assembled_qualified_nominal_without_import() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/exact-assembled-nominal/project/src");
    let linux_path = root.join("Platform/Linux.bd");
    let console_path = root.join("Console/Console.bd");
    let capabilities_path = root.join("Console/Capabilities.bd");
    let linux_source = r#"
pub Console.ConsoleSize Winsize() {
    return Console.ConsoleSize { columns: 80, rows: 24 };
}
"#;
    let console_source = "pub type ConsoleSize { i32 columns, i32 rows }";
    let capabilities_source = "pub type TerminalCapabilities { bool isTty }";
    let sources =
        [(&linux_path, linux_source), (&console_path, console_source), (&capabilities_path, capabilities_source)];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let linux_program = units[0].program.clone();
    let generation = SyntaxGenerationId(66);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let linux_unit = SourceUnitId::new(&db, linux_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        linux_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let linux_index = SyntaxIndex::from_program(&linux_program, generation);
    let winsize = key(linux_unit, generation, &linux_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        item_abi_signature(&db, winsize).expect("Winsize item ABI"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER })
    );
}

#[test]
fn generic_specialization_accepts_qualified_nominal_corelib_test_arguments() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/corelib-generic-specialization/project/src");
    let main_path = root.join("ArgsTests.bd");
    let args_path = root.join("Core/Args/Args.bd");
    let args_error_path = root.join("Core/Args/ArgsError.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let assert_path = root.join("Testing/Assert.bd");
    let main_source = r#"
use Core.Args;
use Core.Results;
use Testing.Assert;

test corelib_generic_specialization {
    Core.Results.Result<string, Core.Args.ArgsError> result = Args.Get(0);
    Assert.True(Results.IsOk(result), "args[0] should be valid");
    string[] all = Args.All();
    i64 len = __array_len(all);
    Assert.Equal(len, Args.Count(), "All() length matches Count()");
}
"#;
    let sources = [
        (main_path.clone(), main_source.to_owned()),
        (
            args_path.clone(),
            "pub Core.Results.Result<string, ArgsError> Get(i64 index) { return Result::Error(ArgsError::IndexOutOfRange()); }\npub string[] All() { return __array_new(8, 0); }\npub i64 Count() { return 1_i64; }"
                .to_owned(),
        ),
        (args_error_path.clone(), "pub enum ArgsError { IndexOutOfRange() }".to_owned()),
        (
            results_path.clone(),
            "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }\npub bool IsOk<TValue, TError>(Result<TValue, TError> value) { return true; }"
                .to_owned(),
        ),
        (
            assert_path.clone(),
            "pub unit Equal<T>(T actual, T expected, string because) { return; }\npub unit True(bool condition, string because) { return; }"
                .to_owned(),
        ),
    ];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: path.clone(),
            source: source.clone(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let main_program = units[0].program.clone();
    let generation = SyntaxGenerationId(65);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);

    let equal = main_index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: main_unit, generation, node })
        .find(|call| call_arguments(&db, *call).expect("call arguments").is_some_and(|arguments| arguments.len() == 3))
        .expect("Assert.Equal call");
    let equal_arguments = call_arguments(&db, equal).expect("Assert.Equal arguments").expect("arguments");
    assert_eq!(
        node_type(&db, equal_arguments[1]).expect("nested direct-call semantic type"),
        Some(SemanticTypeId::I64)
    );

    for node in main_index.ids_of_kind(NodeKind::CallExpression) {
        let call = AstNodeKey { unit: main_unit, generation, node };
        if generic_call_instantiation(&db, call).expect("generic call fact").is_some() {
            assert!(
                generic_call_specialization(&db, call).expect("generic call specialization query").is_some(),
                "generic call at {call:?} must have an ABI specialization"
            );
        }
    }
}

#[test]
fn canonical_core_error_qualified_write_has_a_direct_semantic_fact() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/core-error-qualified-call/project/src");
    let error_path = root.join("Core/Error/Error.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let syscall_path = root.join("Core/Syscall/Syscall.bd");
    let syscall_error_path = root.join("Core/Syscall/SyscallError.bd");
    let descriptor_path = root.join("Core/Syscall/Descriptor.bd");
    let standard_stream_path = root.join("Core/Syscall/StandardStream.bd");
    let write_request_path = root.join("Core/Syscall/WriteRequest.bd");
    let error_source_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src/Core/Error/Error.bd");
    let error_source = std::fs::read_to_string(&error_source_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", error_source_path.display()));
    let sources = [
        (error_path.clone(), error_source),
        (
            results_path.clone(),
            "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }".into(),
        ),
        (
            syscall_path.clone(),
            "pub Core.Results.Result<i64, Core.Syscall.SyscallError> WriteWith(Core.Syscall.WriteRequest request) { return Result::Ok(0_i64); }".into(),
        ),
        (
            syscall_error_path,
            "pub enum SyscallError { IoFailure(i64 code) }".into(),
        ),
        (
            descriptor_path,
            "pub enum Descriptor { Standard(Core.Syscall.StandardStream stream), Raw(i64 fd) }".into(),
        ),
        (
            standard_stream_path,
            "pub enum StandardStream { Stdin, Stdout, Stderr }".into(),
        ),
        (
            write_request_path,
            "pub type WriteRequest { Core.Syscall.Descriptor descriptor, string data }".into(),
        ),
    ];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: path.clone(),
            source: source.clone(),
            program: expand_program(
                parse_program(source).expect("parse Core.Error regression source"),
                DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            ),
        })
        .collect::<Vec<_>>();
    let error_program = units[0].program.clone();
    let syscall_program = units[2].program.clone();
    let generation = SyntaxGenerationId(28);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let error_unit = SourceUnitId::new(&db, error_path);
    let syscall_unit = SourceUnitId::new(&db, syscall_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        error_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let error_index = SyntaxIndex::from_program(&error_program, generation);
    let syscall_index = SyntaxIndex::from_program(&syscall_program, generation);
    let call = key(error_unit, generation, &error_index, NodeKind::CallExpression, 0);
    let declaration = key(syscall_unit, generation, &syscall_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("Core.Error WriteWith call lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn imported_generic_owned_method_call_has_concrete_receiver_specialized_abi() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-owned-method/project/src");
    let main_path = root.join("Main.bd");
    let list_path = root.join("Core/Collections/List.bd");
    let main_source = r#"
use Core.Collections.List;
unit Main() {
    List<i64> values = List<i64> { marker: 0_i64 };
    values.Get(0);
}
"#;
    let list_source = r#"
pub type List<T> {
    i64 marker,

    pub T Get(i64 index) {
        return 0_i64;
    }
}
"#;
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let list_program =
        expand_program(parse_program(list_source).expect("list parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(29);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: list_path.display().to_string(),
                path: list_path.clone(),
                source: list_source.to_string(),
                program: list_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let list_unit = SourceUnitId::new(&db, list_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let list_index = SyntaxIndex::from_program(&list_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(list_unit, generation, &list_index, NodeKind::MethodDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("imported generic method lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        call_abi_signature(&db, call).expect("imported generic method call ABI"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::POINTER, SemanticTypeId::I64]),
            result: SemanticTypeId::I64,
        })
    );
}

#[test]
fn imported_generic_owned_method_result_match_has_concrete_storage_abi() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-owned-method-result/project/src");
    let main_path = root.join("Main.bd");
    let list_path = root.join("Core/Collections/List.bd");
    let result_path = root.join("Core/Results/Results.bd");
    let main_source = r#"
use Core.Collections.List;
unit Main() {
    List<i64> values = List<i64> { marker: 0_i64 };
    i64 first = match values.Get(0) {
        Core.Results.Result::Ok(value) => value,
        Core.Results.Result::Error(_) => -1,
    };
}
"#;
    let list_source = r#"
use Core.Results;
pub type List<T> {
    i64 marker,

    pub Result<T, string> Get(i64 index) {
        return Result::Error("missing");
    }
}
"#;
    let result_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let list_program =
        expand_program(parse_program(list_source).expect("list parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let result_program =
        expand_program(parse_program(result_source).expect("result parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(30);
    let list_unit = SourceUnitId::new(&db, list_path.clone());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: list_path.display().to_string(),
                path: list_path,
                source: list_source.to_string(),
                program: list_program.clone(),
            },
            SourceUnit {
                logical_name: result_path.display().to_string(),
                path: result_path,
                source: result_source.to_string(),
                program: result_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let matched = key(main_unit, generation, &main_index, NodeKind::MatchExpression, 0);
    let negative_arm = key(main_unit, generation, &main_index, NodeKind::UnaryExpression, 0);
    let stored = key(main_unit, generation, &main_index, NodeKind::LetStatement, 1);
    let list_index = SyntaxIndex::from_program(&list_program, generation);
    let constructor = key(list_unit, generation, &list_index, NodeKind::EnumConstructorExpression, 0);

    assert_eq!(
        call_abi_signature(&db, call).expect("imported generic method call ABI"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::POINTER, SemanticTypeId::I64]),
            result: SemanticTypeId::POINTER,
        })
    );
    assert!(enum_match(&db, matched).expect("imported Result match").is_some());
    assert_eq!(node_type(&db, matched).expect("imported Result match type"), Some(SemanticTypeId::I64));
    assert_eq!(value_abi_type(&db, negative_arm).expect("contextual match arm ABI"), Some(SemanticTypeId::I64));
    let template = enum_constructor_template(&db, constructor)
        .expect("generic Result constructor template query")
        .expect("generic Result constructor template");
    assert_eq!(template.parameters.as_ref(), [Arc::<str>::from("TValue"), Arc::<str>::from("TError")]);
    assert_eq!(
        template.arguments.as_ref(),
        [
            EnumLayoutTemplateArgument::EnclosingParameter(Arc::from("T")),
            EnumLayoutTemplateArgument::Concrete(AggregateFieldShape::Scalar(SemanticTypeId::STRING)),
        ]
    );
    let specialized = enum_constructor_specialization(
        &db,
        constructor,
        Arc::from([GenericSubstitution::inferred("T", SemanticTypeId::I64)]),
    )
    .expect("generic Result constructor specialization query")
    .expect("generic Result constructor specialization");
    assert_eq!(specialized.layout.variants[0].fields[0].1, AggregateFieldShape::Scalar(SemanticTypeId::I64));
    assert_eq!(specialized.layout.variants[1].fields[0].1, AggregateFieldShape::Scalar(SemanticTypeId::STRING));
    assert_eq!(value_abi_type(&db, stored).expect("match storage ABI"), Some(SemanticTypeId::I64));
}
