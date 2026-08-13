use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, CallLowering, ItemSignature, SemanticTypeId, abi_type, call_abi_signature, call_arguments,
    call_lowering, generic_call_instantiation, generic_call_specialization, generic_call_template,
    generic_nominal_method_receiver,
};
use std::sync::Arc;

#[test]
fn generic_parameter_type_argument_call_remains_direct_inside_generic_body() {
    let source = r#"
type Channel<T> { i64 handle }
type Options { i64 flags }
Options Default() { return Options { flags: 0_i64 }; }
Channel<T> CreateWithOptions<T>(Options options) { return Channel<T> { handle: options.flags }; }
Channel<T> Create<T>() { return CreateWithOptions<T>(Default()); }
unit Main() { Channel<i64> ch = Create<i64>(); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let create_with_options = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let nested = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .find(|call| {
            matches!(
                call_lowering(&db, *call).ok().flatten(),
                Some(beskid_queries::CallLowering::Direct(declaration))
                    if declaration == create_with_options
            )
        })
        .expect("CreateWithOptions<T> call inside Create");

    assert_eq!(
        generic_call_instantiation(&db, nested).expect("parameter type-arg instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration: create_with_options,
            argument_count: 1,
            arguments: Arc::from([]),
        })
    );
    assert_eq!(
        call_abi_signature(&db, nested).expect("nested call ABI"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER })
    );
}

#[test]
fn concrete_nominal_type_argument_is_not_a_nested_generic_call_template() {
    let source = r#"
type ConsoleMessage { i64 kind }
type Channel<T> { i64 handle }
Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; }
Channel<ConsoleMessage> MessagesChannel() { return Create<ConsoleMessage>(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let create = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let call = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .find(|call| {
            matches!(call_lowering(&db, *call).ok().flatten(), Some(CallLowering::Direct(declaration)) if declaration == create)
        })
        .expect("Create<ConsoleMessage> call");

    assert_eq!(
        generic_call_template(&db, call).expect("concrete nominal type argument query"),
        None,
        "only a type argument bound by the enclosing generic declaration is a deferred template"
    );
    assert!(
        generic_call_specialization(&db, call).expect("concrete nominal generic specialization").is_some(),
        "the concrete nominal call must be specialized directly"
    );
}

#[test]
fn explicit_generic_aggregate_argument_uses_the_declared_parameter_abi() {
    let source = r#"
type SendOk { }
type ChannelError { }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
Result<TValue, TError> Success<TValue, TError>(TValue value) { return Result::Ok(value); }
Result<SendOk, ChannelError> MapSendStatus() { return Success<SendOk, ChannelError>(SendOk { }); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let success = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let call = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .find(|call| {
            matches!(call_lowering(&db, *call).ok().flatten(), Some(CallLowering::Direct(declaration)) if declaration == success)
        })
        .expect("Success<SendOk, ChannelError> call");

    assert_eq!(
        generic_call_specialization(&db, call).expect("explicit aggregate specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: success,
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::POINTER]),
                result: SemanticTypeId::POINTER,
            },
            substitutions: Arc::from([
                beskid_queries::GenericSubstitution {
                    parameter: Arc::from("TValue"),
                    argument: SemanticTypeId::POINTER,
                },
                beskid_queries::GenericSubstitution {
                    parameter: Arc::from("TError"),
                    argument: SemanticTypeId::POINTER,
                },
            ]),
        })
    );
}

#[test]
fn enclosing_generic_parameter_argument_keeps_an_imported_generic_call_direct() {
    let source = r#"
type ChannelError { }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
Result<TValue, TError> Success<TValue, TError>(TValue value) { return Result::Ok(value); }
Result<T, ChannelError> Receive<T>(T value) { return Success<T, ChannelError>(value); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let success = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("enclosing generic argument lowering"),
        Some(CallLowering::Direct(success))
    );
}

#[test]
fn inferred_generic_call_has_an_exact_argument_derived_abi_signature() {
    let source = r#"
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main() { Equal(1, 1, "because"); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let arguments = call_arguments(&db, call).expect("generic arguments").expect("generic arguments available");
    assert_eq!(abi_type(&db, arguments[0]), Ok(Some(SemanticTypeId::I32)));
    assert_eq!(abi_type(&db, arguments[1]), Ok(Some(SemanticTypeId::I32)));
    assert_eq!(abi_type(&db, arguments[2]), Ok(Some(SemanticTypeId::STRING)));

    assert_eq!(
        call_abi_signature(&db, call).expect("inferred generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I32, SemanticTypeId::I32, SemanticTypeId::STRING,]),
            result: SemanticTypeId::UNIT,
        })
    );
    assert_eq!(
        generic_call_specialization(&db, call).expect("inferred generic specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: key(unit, generation, &index, NodeKind::FunctionDefinition, 0),
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::I32, SemanticTypeId::I32, SemanticTypeId::STRING,]),
                result: SemanticTypeId::UNIT,
            },
            substitutions: Arc::from([beskid_queries::GenericSubstitution {
                parameter: Arc::from("T"),
                argument: SemanticTypeId::I32,
            }]),
        })
    );
}

#[test]
fn generic_call_specializes_an_enum_pattern_binding_by_its_payload_abi() {
    let source = r#"
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main(Result<string, string> result) {
    match result {
        Result::Ok(text) => { Equal(text, "ok", "message"); },
        Result::Error(_) => {},
    };
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let equal = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        generic_call_specialization(&db, call).expect("pattern-binding generic specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: equal,
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::STRING, SemanticTypeId::STRING, SemanticTypeId::STRING]),
                result: SemanticTypeId::UNIT,
            },
            substitutions: Arc::from([beskid_queries::GenericSubstitution {
                parameter: Arc::from("T"),
                argument: SemanticTypeId::STRING,
            }]),
        })
    );
}

#[test]
fn generic_nominal_method_specializes_from_its_explicit_receiver_application() {
    let source = r#"
type List<T> {
    T value,
    T Echo(T input) { return input; }
}
unit Main(List<i64> list) { list.Echo(1_i64); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("generic nominal method lowering"),
        Some(CallLowering::Direct(method))
    );
    let receiver = generic_nominal_method_receiver(&db, call)
        .expect("generic nominal receiver")
        .expect("explicit List<i64> receiver must prove the owner environment");
    assert_eq!(receiver.method, method);
    assert_eq!(receiver.owner, key(unit, generation, &index, NodeKind::TypeDefinition, 0));
    assert_eq!(
        receiver.substitutions,
        Arc::from([beskid_queries::GenericSubstitution {
            parameter: Arc::from("T"),
            argument: SemanticTypeId::I64,
        }])
    );

    assert_eq!(
        generic_call_specialization(&db, call).expect("generic nominal method specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: method,
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::POINTER, SemanticTypeId::I64]),
                result: SemanticTypeId::I64,
            },
            substitutions: Arc::from([beskid_queries::GenericSubstitution {
                parameter: Arc::from("T"),
                argument: SemanticTypeId::I64,
            }]),
        })
    );
}

#[test]
fn generic_nominal_method_owner_binds_nested_generic_templates() {
    let source = r#"
T Id<T>(T value) { return value; }
type List<T> {
    T value,
    T Echo(T input) { return Id<T>(input); }
}
unit Main(List<i64> list) { list.Echo(1_i64); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let id = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let nested = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        generic_call_template(&db, nested).expect("generic method nested template"),
        Some(beskid_queries::GenericCallTemplate {
            declaration: id,
            parameters: Arc::from([Arc::from("T")]),
            parameter_arguments: Arc::from([Arc::from("T")]),
        })
    );
}

#[test]
fn inferred_generic_call_allows_a_bare_integer_to_follow_an_exact_i64_argument() {
    let source = r#"
i64 Position() { return 0_i64; }
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main() { Equal(Position(), 0, "initial position"); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(
        call_abi_signature(&db, call).expect("nested generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64, SemanticTypeId::STRING,]),
            result: SemanticTypeId::UNIT,
        })
    );
}

#[test]
fn inferred_generic_call_allows_a_negative_bare_integer_to_follow_an_exact_i64_argument() {
    let source = r#"
i64 Position() { return 0_i64; }
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main() { Equal(Position(), -1, "negative position"); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_abi_signature(&db, call).expect("negative-literal generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64, SemanticTypeId::STRING,]),
            result: SemanticTypeId::UNIT,
        })
    );
    assert_eq!(
        generic_call_specialization(&db, call).expect("negative-literal generic specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: key(unit, generation, &index, NodeKind::FunctionDefinition, 1),
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64, SemanticTypeId::STRING,]),
                result: SemanticTypeId::UNIT,
            },
            substitutions: Arc::from([beskid_queries::GenericSubstitution {
                parameter: Arc::from("T"),
                argument: SemanticTypeId::I64,
            }]),
        })
    );
}

#[test]
fn inferred_generic_call_does_not_rebind_an_explicit_integer_suffix() {
    for (bound_type, explicit_literal) in [("i64", "0_i32"), ("i32", "0_i64"), ("i32", "0_u8")] {
        let source = format!(
            r#"
{bound_type} Position() {{ return 0_{bound_type}; }}
unit Equal<T>(T actual, T expected, string because) {{ return; }}
unit Main() {{ Equal(Position(), {explicit_literal}, "initial position"); return; }}
"#
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

        assert_unavailable(call_abi_signature(&db, call));
    }
}

#[test]
fn nominal_generic_types_have_only_source_derived_pointer_abi_facts() {
    let source = r#"
type Channel<T> { i64 handle }
type Pair<T> { i64 left, i64 right }
Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; }
Pair<T> CreatePair<T>() { return Pair<T> { left: 0_i64, right: 0_i64 }; }
unit Main() {
    Channel<i64> channel = Create<i64>();
    Pair<i64> pair = CreatePair<i64>();
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let create = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let create_pair = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let channel_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let pair_call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let channel_let = key(unit, generation, &index, NodeKind::LetStatement, 0);
    let pair_let = key(unit, generation, &index, NodeKind::LetStatement, 1);

    assert_eq!(
        beskid_queries::item_abi_signature(&db, create).expect("generic item has no fixed ABI"),
        None,
        "generic factories must not publish a single item ABI; callers specialize"
    );
    assert_eq!(beskid_queries::abi_type(&db, channel_call).expect("nominal call ABI"), Some(SemanticTypeId::POINTER));
    assert_eq!(beskid_queries::abi_type(&db, channel_let).expect("nominal local ABI"), Some(SemanticTypeId::POINTER));
    assert_eq!(
        beskid_queries::item_abi_signature(&db, create_pair).expect("generic multi-field factory has no fixed ABI"),
        None
    );
    assert_eq!(
        beskid_queries::abi_type(&db, pair_call).expect("multi-field nominal call ABI"),
        Some(SemanticTypeId::POINTER)
    );
    assert_eq!(
        beskid_queries::abi_type(&db, pair_let).expect("multi-field nominal local ABI"),
        Some(SemanticTypeId::POINTER)
    );
}
