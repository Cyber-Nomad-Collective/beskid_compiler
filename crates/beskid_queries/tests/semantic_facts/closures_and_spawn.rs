use super::support::{key, key_at_start, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    CaptureStorageClass, ClosureAllocationStatus, ClosureCallTarget, ClosureCapture, ClosureEnvironmentField,
    ClosureLoweringStatus, ClosurePointerMapRequirement, ItemSignature, SemanticTypeId, SpawnDiagnosticKind,
    SpawnEntryValidation, SyntaxGenerationId, callable_signature, capture_storage, closure_call_target,
    closure_environment, closure_signature, local_slot, node_kind, node_span, spawn_entry_validation, spawn_legality,
    spawn_target,
};
use std::sync::Arc;

#[test]
fn removed_parent_owned_cancel_slot_builtin_is_unavailable() {
    assert!(beskid_analysis::builtins::builtin_for_path(&["__fiber_spawn_with_cancel_slot".into()]).is_none());
}

#[test]
fn inferred_spawn_handle_preserves_nominal_receiver_and_result_identity() {
    let source = "pub type Fiber<T> { i64 handle, pub unit Cancel() { return; } } i64 Compute() { return 42_i64; } unit Main() { let child = spawn Compute(); child.Cancel(); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    assert_eq!(beskid_queries::abi_type(&db, spawn).unwrap(), Some(SemanticTypeId::POINTER));
    let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let receiver =
        beskid_queries::generic_nominal_method_receiver(&db, call).unwrap().expect("inferred Fiber receiver");
    assert_eq!(receiver.substitutions[0].argument, SemanticTypeId::I64);
}

#[test]
fn spawn_heap_array_capture_is_transferable_but_native_pointer_is_not() {
    for (ty, legal) in [("u8[]", true), ("pointer", false)] {
        let source = format!("i32 Main({ty} buffer) {{ let child = spawn (() => buffer); return 0; }}");
        let (db, _project, unit, generation, index) = setup(&source);
        let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
        let fact = spawn_legality(&db, spawn).unwrap().unwrap();
        assert_eq!(fact.is_legal(), legal, "{ty}: {fact:?}");
    }
}

#[test]
fn moved_and_returned_fiber_bindings_preserve_the_nominal_receiver() {
    for body in [
        "let child = spawn Compute(); let next = child; next.Cancel();",
        "let child = Start(); child.Cancel();",
        "let next = parameter; next.Cancel();",
    ] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, pub unit Cancel() {{ return; }} }} i64 Compute() {{ return 42_i64; }} Fiber<i64> Start() {{ return spawn Compute(); }} unit Main(Fiber<i64> parameter) {{ {body} return; }}"
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let call = beskid_queries::AstNodeKey {
            unit,
            generation,
            node: index.ids_of_kind(NodeKind::CallExpression).last().unwrap(),
        };
        let receiver = beskid_queries::generic_nominal_method_receiver(&db, call)
            .unwrap()
            .expect("inferred Fiber receiver after ownership transfer");
        assert_eq!(receiver.substitutions[0].argument, SemanticTypeId::I64, "{body}");
    }
}

#[test]
fn spawn_handle_terminal_ownership_is_checked_from_current_syntax() {
    for (body, expected) in [
        ("spawn Compute();", Some("DiscardedHandle")),
        ("let child = spawn Compute(); child.Join(); child.Join();", Some("UseAfterMove")),
        ("let child = spawn Compute(); child.Detach(); child.Join();", Some("UseAfterMove")),
        ("let child = spawn Compute(); child.Cancel(); child.Cancel(); child.Join();", None),
        ("let child = spawn Compute(); child.Detach();", None),
        ("let child = spawn Compute(); let next = child; child.Join();", Some("UseAfterMove")),
        ("let child = spawn Compute(); let next = child; next.Join(); next.Join();", Some("UseAfterMove")),
        ("let child = spawn Compute(); let next = child; next.Cancel(); next.Join();", None),
        ("let child = spawn Compute(); if true { child.Join(); } else { child.Detach(); }", None),
        ("let child = spawn Compute(); if true { child.Join(); } child.Join();", Some("UseAfterMove")),
        ("let child = spawn Compute(); while true { child.Join(); }", Some("UseAfterMove")),
    ] {
        let source = format!("i64 Compute() {{ return 42; }} unit Main() {{ {body} return; }}");
        let (db, _project, unit, generation, index) = setup(&source);
        let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
        let fact = spawn_legality(&db, spawn).unwrap().unwrap();
        match expected {
            Some(expected) => assert!(format!("{:?}", fact.diagnostics).contains(expected), "{body}: {fact:?}"),
            None => assert!(fact.is_legal(), "{body}: {fact:?}"),
        }
    }
}

#[test]
fn closure_environment_reports_only_outer_lexical_captures() {
    let source = r#"i32 Main(i32 outer) {
    let copied = outer;
    let apply = (i32 inner) => copied + inner;
    return apply(1);
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let copied_offset = source.find("copied =").expect("copied declaration");
    let copied = key_at_start(unit, generation, &index, NodeKind::Identifier, copied_offset);
    let copied_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("copied +").expect("copied capture use"),
    );
    let inner_offset = source.find("inner) =>").expect("lambda parameter");
    let inner = key_at_start(unit, generation, &index, NodeKind::Identifier, inner_offset);

    let closure = closure_environment(&db, lambda).expect("closure environment").expect("lambda fact");
    assert_eq!(closure.parameters.as_ref(), &[inner]);
    assert_eq!(
        closure.captures.as_ref(),
        &[ClosureCapture {
            declaration: copied,
            slot: local_slot(&db, copied).expect("outer local slot").expect("outer local slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, copied_use).expect("copied use span").expect("copied use span fact"),
        }]
    );
}

#[test]
fn spawn_legality_projects_ownership_of_returned_and_parameter_handles() {
    for body in ["let child = Start(); child.Join(); child.Join();", "parameter.Detach(); parameter.Join();"] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, }} i64 Compute() {{ return 42_i64; }} Fiber<i64> Start() {{ return spawn Compute(); }} unit Main(Fiber<i64> parameter) {{ let marker = spawn Compute(); {body} return; }}"
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 1);
        let fact = spawn_legality(&db, spawn).unwrap().unwrap();
        assert!(
            fact.diagnostics.iter().any(|diagnostic| diagnostic.kind == SpawnDiagnosticKind::UseAfterMove),
            "{body}: {fact:?}"
        );
    }
}

#[test]
fn callable_fiber_ownership_covers_handles_without_local_spawn() {
    for (body, rejected) in [
        ("let child = Start(); child.Join(); child.Join();", true),
        ("parameter.Detach(); parameter.Join();", true),
        ("parameter.Join(); parameter.Cancel();", true),
        ("let next = parameter; parameter.Join();", true),
        ("let next = parameter; next.Join(); next.Detach();", true),
        ("let next = parameter; next.Cancel(); next.Join();", false),
        ("let child = Start(); if true { child.Join(); } else { child.Detach(); }", false),
        ("if true { parameter.Join(); } else { parameter.Detach(); }", false),
        ("if true { parameter.Join(); } parameter.Join();", true),
        ("parameter.Cancel(); parameter.Cancel(); parameter.Join();", false),
    ] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, }} Fiber<i64> Start() {{ return Fiber<i64> {{ handle: 1_i64 }}; }} unit Main(Fiber<i64> parameter) {{ {body} return; }}"
        );
        let (db, _project, unit, generation, index) = setup(&source);
        assert_eq!(index.ids_of_kind(NodeKind::SpawnExpression).count(), 0);
        let callable = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
        let fact = beskid_queries::callable_fiber_ownership(&db, callable).unwrap().unwrap();
        assert_eq!(fact.callable, callable);
        assert_eq!(
            fact.diagnostics.iter().any(|diagnostic| diagnostic.kind == SpawnDiagnosticKind::UseAfterMove),
            rejected,
            "{body}: {fact:?}"
        );
        assert!(
            beskid_queries::callable_fiber_ownership(
                &db,
                beskid_queries::AstNodeKey { generation: SyntaxGenerationId(generation.0 + 1), ..callable }
            )
            .unwrap()
            .is_none()
        );
    }
}

#[test]
fn fiber_ownership_rejects_repeatable_closures_consuming_captures() {
    for (body, rejected) in [
        ("let run = () => child.Join(); run(); run();", true),
        ("let run = () => child.Detach(); run();", true),
        ("let run = () => child; let next = run;", true),
        ("let run = () => child.Cancel(); run(); run(); child.Join();", false),
        ("let run = () => 42_i64; run(); run(); child.Join();", false),
        ("let next = spawn (() => child.Join());", false),
    ] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, pub i64 Join() {{ return 1_i64; }} pub unit Cancel() {{ return; }} }} i64 Compute() {{ return 42_i64; }} unit Main() {{ let child = spawn Compute(); {body} return; }}"
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let callable = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
        let fact = beskid_queries::callable_fiber_ownership(&db, callable).unwrap().unwrap();
        assert_eq!(
            fact.diagnostics.iter().any(|d| d.kind == SpawnDiagnosticKind::UseAfterMove),
            rejected,
            "{body}: {fact:?}"
        );
    }
}

#[test]
fn fiber_ownership_tracks_inferred_factory_method_results() {
    for (factory, parameter) in [
        (
            "pub type Factory { i64 value, pub Fiber<i64> Start() { return Fiber<i64> { handle: 1_i64 }; } }",
            "Factory factory",
        ),
        (
            "pub type Factory<T> { T value, pub Fiber<T> Start() { return Fiber<T> { handle: 1_i64 }; } }",
            "Factory<i64> factory",
        ),
        ("pub type Factory<T> { T value, pub T Start() { return this.value; } }", "Factory<Fiber<i64>> factory"),
    ] {
        for (body, rejected) in [
            ("child.Join(); child.Join();", true),
            ("let next = child; next.Detach(); next.Join();", true),
            ("child.Cancel(); child.Cancel(); child.Join();", false),
            ("if true { child.Join(); } else { child.Detach(); }", false),
        ] {
            let source = format!(
                "pub type Fiber<T> {{ i64 handle, }} {factory} unit Main({parameter}) {{ let child = factory.Start(); {body} return; }}"
            );
            let (db, _project, unit, generation, index) = setup(&source);
            let callable = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
            let fact = beskid_queries::callable_fiber_ownership(&db, callable).unwrap().unwrap();
            assert_eq!(
                fact.diagnostics.iter().any(|d| d.kind == SpawnDiagnosticKind::UseAfterMove),
                rejected,
                "{parameter}, {body}: {fact:?}"
            );
        }
    }
}

#[test]
fn fiber_ownership_does_not_invent_capabilities_for_other_factory_results() {
    let source = "pub type Fiber<T> { i64 handle, } pub type Value { i64 raw, pub unit Join() { return; } } pub type Factory { i64 raw, pub Value Start() { return Value { raw: 1_i64 }; } } unit Main(Factory factory) { let value = factory.Start(); value.Join(); value.Join(); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let callable = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert!(beskid_queries::callable_fiber_ownership(&db, callable).unwrap().unwrap().diagnostics.is_empty());
}

#[test]
fn fiber_source_identity_does_not_depend_on_terminal_ownership_legality() {
    let source = "pub type Fiber<T> { i64 handle, pub T Join() { return 1_i64; } } i64 Compute() { return 42_i64; } unit Main() { let child = spawn Compute(); let value = child.Join(); child.Join(); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    assert!(beskid_queries::spawn_handle_type(&db, spawn).unwrap().is_some());
    let callable = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let fact = beskid_queries::callable_fiber_ownership(&db, callable).unwrap().unwrap();
    assert!(fact.diagnostics.iter().any(|d| d.kind == SpawnDiagnosticKind::UseAfterMove));
}

#[test]
fn closure_contract_is_generation_bound_and_requires_a_pointer_map_without_claiming_lowering() {
    let source = r#"i32 Main(i32 first, i32 second, string message) {
    let sum = () => first + second;
    let text = () => message;
    return sum();
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let sum = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let text = key(unit, generation, &index, NodeKind::LambdaExpression, 1);
    let first =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("first,").expect("first parameter"));
    let second =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("second,").expect("second parameter"));
    let message = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("message)").expect("message parameter"),
    );
    let sum_body = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let first_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> first +").expect("first capture use") + "=> ".len(),
    );
    let second_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("first + second").expect("second capture use") + "first + ".len(),
    );
    let message_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> message").expect("message reference") + "=> ".len(),
    );

    let sum_contract = closure_signature(&db, sum).expect("sum closure contract").expect("sum closure fact");
    assert_eq!(sum_contract.lambda, sum);
    assert_eq!(sum_contract.lambda.generation, generation);
    assert_eq!(sum_contract.body, sum_body);
    assert_eq!(sum_contract.callable, ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I32 });
    assert_eq!(
        sum_contract.environment.fields.as_ref(),
        &[
            ClosureEnvironmentField {
                capture: ClosureCapture {
                    declaration: first,
                    slot: local_slot(&db, first).expect("first slot").expect("first slot fact"),
                    class: CaptureStorageClass::TransferableValue,
                    span: node_span(&db, first_use).expect("first use span").expect("first use span fact"),
                },
                abi_type: SemanticTypeId::I32,
            },
            ClosureEnvironmentField {
                capture: ClosureCapture {
                    declaration: second,
                    slot: local_slot(&db, second).expect("second slot").expect("second slot fact"),
                    class: CaptureStorageClass::TransferableValue,
                    span: node_span(&db, second_use).expect("second use span").expect("second use span fact"),
                },
                abi_type: SemanticTypeId::I32,
            },
        ]
    );
    assert_eq!(sum_contract.environment.pointer_map, ClosurePointerMapRequirement::RuntimeDescriptorRequired);
    assert_eq!(sum_contract.lowering, ClosureLoweringStatus::NotLowered);
    assert_eq!(sum_contract.allocation, ClosureAllocationStatus::NotAllocated);

    let text_contract = closure_signature(&db, text).expect("text closure contract").expect("text closure fact");
    assert_eq!(text_contract.body, message_reference);
    assert_eq!(
        text_contract.environment.fields.as_ref(),
        &[ClosureEnvironmentField {
            capture: ClosureCapture {
                declaration: message,
                slot: local_slot(&db, message).expect("message slot").expect("message slot fact"),
                class: CaptureStorageClass::TransferableValue,
                span: node_span(&db, message_reference).expect("message use span").expect("message use span fact"),
            },
            abi_type: SemanticTypeId::STRING,
        }]
    );
    assert_eq!(text_contract.environment.pointer_map, ClosurePointerMapRequirement::RuntimeDescriptorRequired);
}

#[test]
fn closure_call_target_and_spawn_entry_validation_use_only_current_syntax_facts() {
    let call_source = "i32 Main() { return ((i32 value) => value)(7); }";
    let (db, _project, unit, generation, index) = setup(call_source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let body = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        call_source.find("=> value").expect("lambda body") + "=> ".len(),
    );
    assert_eq!(
        closure_call_target(&db, call).expect("closure call target"),
        Some(ClosureCallTarget {
            call,
            lambda,
            body,
            callable: ItemSignature { parameters: Arc::from([SemanticTypeId::I32]), result: SemanticTypeId::I32 },
        })
    );

    let spawn_source = "i32 Main() { let task = spawn (() => 7); return 0; }";
    let (db, _project, unit, generation, index) = setup(spawn_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    assert_eq!(
        spawn_entry_validation(&db, spawn).expect("spawn entry validation"),
        Some(SpawnEntryValidation {
            spawn,
            target: lambda,
            callable: Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I32 }),
            is_zero_argument_entry: true,
            diagnostics: Arc::from([]),
        })
    );
}

#[test]
fn stored_lambda_call_resolves_its_lexical_initializer_without_hir() {
    let source = "i32 Main() { let add = (i32 value) => value + 1; return add(7); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let body = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::BinaryExpression,
        source.find("value + 1").expect("lambda body"),
    );

    assert_eq!(
        closure_call_target(&db, call).expect("stored closure call target"),
        Some(ClosureCallTarget {
            call,
            lambda,
            body,
            callable: ItemSignature { parameters: Arc::from([SemanticTypeId::I32]), result: SemanticTypeId::I32 },
        })
    );
}

#[test]
fn spawn_target_preserves_lambda_operand_and_capture_environment() {
    let source = r#"i32 Main(i32 outer) {
    let task = spawn ((i32 inner) => outer + inner);
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let outer_offset = source.find("outer)").expect("parameter declaration");
    let outer = key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offset);
    let outer_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer +").expect("outer capture use") + "=> ".len(),
    );

    let spawn = spawn_target(&db, spawn).expect("spawn target").expect("spawn fact");
    assert_eq!(spawn.callee, lambda);
    assert_eq!(
        spawn.captures.as_ref(),
        &[ClosureCapture {
            declaration: outer,
            slot: local_slot(&db, outer).expect("parameter slot").expect("parameter slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, outer_use).expect("outer use span").expect("outer use span fact"),
        }]
    );
}

#[test]
fn spawn_legality_reports_current_callable_result_and_precise_span() {
    let source = r#"i64 Worker() { return 7_i64; }
i32 Main() { let task = spawn Worker; return 0; }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let worker = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);

    assert_eq!(
        callable_signature(&db, worker).expect("worker signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I64 })
    );

    let legality = spawn_legality(&db, spawn).expect("spawn legality").expect("current spawn fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I64));
    assert_eq!(legality.span, node_span(&db, spawn).expect("spawn span").expect("span"));
    assert!(legality.diagnostics.is_empty());
}

#[test]
fn capture_storage_tracks_nested_shadowed_reference_with_its_exact_span() {
    let source = r#"i32 Main(i32 outer) {
    let make = (i32 outer) => (i32 inner) => outer;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let captured_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer;").map(|offset| offset + "=> ".len()).expect("nested capture reference"),
    );
    let shadowing_parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("outer) =>").expect("shadowing parameter"),
    );

    let capture = capture_storage(&db, captured_reference).expect("capture storage").expect("capture fact");
    assert_eq!(capture.declaration, shadowing_parameter);
    assert_eq!(capture.class, CaptureStorageClass::TransferableValue);
    assert_eq!(capture.span, node_span(&db, captured_reference).expect("reference span").expect("reference span fact"));
}

#[test]
fn closure_environment_reports_nested_shadowed_captures_with_modes_and_spans() {
    let source = r#"i32 Main(i32 outer) {
    let make = (i32 outer) => (i32 inner) => outer;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let outer_lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let inner_lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 1);
    let shadowing_parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("outer) =>").expect("shadowing parameter"),
    );
    let captured_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer;").map(|offset| offset + "=> ".len()).expect("nested capture reference"),
    );

    let outer_environment =
        closure_environment(&db, outer_lambda).expect("outer closure environment").expect("outer lambda fact");
    assert!(
        outer_environment.captures.is_empty(),
        "outer lambda binds shadowing outer and must not capture Main's parameter"
    );

    let inner_environment =
        closure_environment(&db, inner_lambda).expect("inner closure environment").expect("inner lambda fact");
    assert_eq!(
        inner_environment.captures.as_ref(),
        &[ClosureCapture {
            declaration: shadowing_parameter,
            slot: local_slot(&db, shadowing_parameter).expect("shadowing slot").expect("shadowing slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, captured_reference).expect("reference span").expect("reference span fact"),
        }]
    );
}

#[test]
fn spawn_legality_rejects_non_callable_and_stack_capture_with_precise_diagnostics() {
    let non_callable_source = "i32 Main() { let task = spawn 7; return 0; }";
    let (db, _project, unit, generation, index) = setup(non_callable_source);
    let non_callable_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let non_callable =
        spawn_legality(&db, non_callable_spawn).expect("non-callable spawn legality").expect("non-callable spawn fact");
    assert!(!non_callable.is_legal());
    assert_eq!(non_callable.result, None);
    assert_eq!(non_callable.diagnostics.len(), 1);
    assert_eq!(non_callable.diagnostics[0].kind, SpawnDiagnosticKind::TargetNotCallable);
    assert_eq!(
        non_callable.diagnostics[0].span,
        node_span(&db, non_callable_spawn).expect("spawn span").expect("spawn span fact")
    );

    let parameterized_source = "i32 Main() { let task = spawn ((i32 value) => value); return 0; }";
    let (db, _project, unit, generation, index) = setup(parameterized_source);
    let parameterized_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let parameterized = spawn_legality(&db, parameterized_spawn)
        .expect("parameterized spawn legality")
        .expect("parameterized spawn fact");
    assert!(!parameterized.is_legal());
    assert_eq!(parameterized.result, Some(SemanticTypeId::I32));
    assert_eq!(parameterized.diagnostics.len(), 1);
    assert_eq!(parameterized.diagnostics[0].kind, SpawnDiagnosticKind::TargetRequiresArguments);
    assert_eq!(
        parameterized.diagnostics[0].span,
        node_span(&db, parameterized_spawn).expect("parameterized spawn span").expect("parameterized spawn span fact")
    );

    let capture_source = r#"i32 Main(pointer frame) {
    let task = spawn (() => frame);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(capture_source);
    let capture_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let capture_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        capture_source.rfind("frame)").expect("captured pointer reference"),
    );
    let capture =
        capture_storage(&db, capture_reference).expect("pointer capture storage").expect("pointer capture fact");
    assert_eq!(capture.class, CaptureStorageClass::StackReference);

    let illegal_capture =
        spawn_legality(&db, capture_spawn).expect("capturing spawn legality").expect("capturing spawn fact");
    assert!(!illegal_capture.is_legal());
    assert_eq!(illegal_capture.result, Some(SemanticTypeId::POINTER));
    assert_eq!(illegal_capture.diagnostics.len(), 1);
    assert_eq!(illegal_capture.diagnostics[0].kind, SpawnDiagnosticKind::StackReferenceEscapesSpawn);
    assert_eq!(illegal_capture.diagnostics[0].span, capture.span);
}

#[test]
fn stale_generation_never_reuses_spawn_legality_or_capture_storage() {
    let source = r#"i32 Main(i32 value) {
    let task = spawn (() => value);
    return 0;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let capture_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("value)").expect("capture reference"),
    );
    assert!(spawn_legality(&db, spawn).expect("current spawn").is_some());
    assert!(capture_storage(&db, capture_reference).expect("current capture").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("registered syntax edit");

    assert_eq!(spawn_legality(&db, spawn).expect("stale spawn"), None);
    assert_eq!(capture_storage(&db, capture_reference).expect("stale capture"), None);
}

#[test]
fn spawn_legality_normalizes_empty_call_entries_and_rejects_call_arguments() {
    let empty_call_source = r#"i64 Worker() { return 7_i64; }
i32 Main() { let task = spawn Worker(); return 0; }"#;
    let (db, _project, unit, generation, index) = setup(empty_call_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let worker_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        empty_call_source.find("spawn Worker()").map(|offset| offset + "spawn ".len()).expect("empty-call Worker path"),
    );

    let target = spawn_target(&db, spawn).expect("empty-call spawn target").expect("empty-call spawn fact");
    assert_ne!(target.callee, call, "empty-arg spawn call must not keep the CallExpression as the fiber entry");
    assert_eq!(target.callee, worker_path, "empty-arg spawn call must unwrap to the entry path operand");
    assert_eq!(node_kind(&db, target.callee).expect("entry kind"), Some(NodeKind::PathExpression));
    assert!(target.captures.is_empty());

    let legality = spawn_legality(&db, spawn).expect("empty-call spawn legality").expect("empty-call legality fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I64));
    assert_eq!(
        legality.span,
        node_span(&db, spawn).expect("empty-call spawn span").expect("empty-call spawn span fact")
    );

    let entry = spawn_entry_validation(&db, spawn).expect("empty-call spawn entry").expect("empty-call entry fact");
    assert_eq!(entry.target, worker_path);
    assert_eq!(entry.callable, Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I64 }));
    assert!(entry.is_zero_argument_entry);

    let args_source = r#"i64 Worker(i64 value) { return value; }
i32 Main() { let task = spawn Worker(7_i64); return 0; }"#;
    let (db, _project, unit, generation, index) = setup(args_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    let target = spawn_target(&db, spawn).expect("argful spawn target").expect("argful spawn fact");
    assert_eq!(target.callee, call, "spawn call arguments stay on the CallExpression so legality can fail closed");

    let legality = spawn_legality(&db, spawn).expect("argful spawn legality").expect("argful legality fact");
    assert!(!legality.is_legal());
    assert_eq!(legality.result, None);
    assert_eq!(legality.diagnostics.len(), 1);
    assert_eq!(legality.diagnostics[0].kind, SpawnDiagnosticKind::CalleeArgumentsUnsupported);
    assert_eq!(
        legality.diagnostics[0].span,
        node_span(&db, spawn).expect("argful spawn span").expect("argful spawn span fact")
    );
}

#[test]
fn spawn_legality_accepts_transferable_captures_and_rejects_mutable_stack_escapes() {
    let legal_source = r#"i32 Main(i32 value) {
    let task = spawn (() => value);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(legal_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let value = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        legal_source.find("value)").expect("parameter declaration"),
    );
    let value_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        legal_source.find("=> value)").map(|offset| offset + "=> ".len()).expect("transferable capture use"),
    );

    let legality =
        spawn_legality(&db, spawn).expect("transferable spawn legality").expect("transferable legality fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I32));
    assert_eq!(
        legality.target.captures.as_ref(),
        &[ClosureCapture {
            declaration: value,
            slot: local_slot(&db, value).expect("value slot").expect("value slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, value_use).expect("value use span").expect("value use span fact"),
        }]
    );

    let mutable_source = r#"i32 Main() {
    mut i32 frame = 1;
    let task = spawn (() => frame);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(mutable_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let frame_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        mutable_source.rfind("frame)").expect("mutable capture reference"),
    );
    let capture = capture_storage(&db, frame_use).expect("mutable capture storage").expect("mutable capture fact");
    assert_eq!(capture.class, CaptureStorageClass::StackReference);

    let legality = spawn_legality(&db, spawn).expect("mutable spawn legality").expect("mutable legality fact");
    assert!(!legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I32));
    assert_eq!(legality.diagnostics.len(), 1);
    assert_eq!(legality.diagnostics[0].kind, SpawnDiagnosticKind::StackReferenceEscapesSpawn);
    assert_eq!(legality.diagnostics[0].span, capture.span);
    assert_eq!(legality.diagnostics[0].capture, Some(capture));
}

#[test]
fn stale_generation_never_reuses_closure_contract_or_spawn_entry_validation() {
    let source = r#"i32 Main(i32 value) {
    let closure = () => value;
    let task = spawn (() => value);
    return closure();
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let closure = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    assert!(closure_signature(&db, closure).expect("current closure").is_some());
    assert!(spawn_entry_validation(&db, spawn).expect("current spawn entry").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("registered syntax edit");

    assert_eq!(closure_signature(&db, closure).expect("stale closure"), None);
    assert_eq!(spawn_entry_validation(&db, spawn).expect("stale spawn entry"), None);
}
