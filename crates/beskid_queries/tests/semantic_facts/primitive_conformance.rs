//! Primitive types conform to contracts through `impl <primitive> : Contract` blocks: the bound is
//! proven, the bounded member call dispatches to the primitive's own method with the receiver
//! passed by value, and malformed primitive implementations are E1611.

use super::support::{key, setup};
use beskid_analysis::syntax::AstNodeId;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, SemanticTypeId, UnitObligationKind, generic_call_specialization,
    generic_call_specialization_in_environment, generic_call_specialization_instance, item_abi_signature,
    method_receiver_abi_type, unit_obligations,
};

/// `SizeOf`'s own member call is the first call of the source; the call in `Main` is the second.
const SIZED: &str = "contract Sized { i64 Size(); } \
    impl i32 : Sized { pub i64 Size() { return 4; } } \
    impl i64 : Sized { pub i64 Size() { return 8; } } \
    impl u8 : Sized { pub i64 Size() { return 1; } } \
    impl f64 : Sized { pub i64 Size() { return 8; } } \
    impl bool : Sized { pub i64 Size() { return 1; } } \
    impl string : Sized { pub i64 Size() { return 16; } } \
    i64 SizeOf<T>(T value) where T: Sized { return value.Size(); } ";

const PRIMITIVES: [(&str, &str, SemanticTypeId); 6] = [
    ("i32", "3_i32", SemanticTypeId::I32),
    ("i64", "4_i64", SemanticTypeId::I64),
    ("u8", "5_u8", SemanticTypeId::U8),
    ("f64", "1.5_f64", SemanticTypeId::F64),
    ("bool", "true", SemanticTypeId::BOOL),
    ("string", "\"abc\"", SemanticTypeId::STRING),
];

#[test]
fn where_bounds_are_proven_for_primitive_type_arguments() {
    for (name, argument, abi) in PRIMITIVES {
        let source = format!("{SIZED}i64 Main() {{ return SizeOf<{name}>({argument}); }}");
        let (db, _, unit, generation, index) = setup(&source);
        let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
        let specialization = generic_call_specialization(&db, call)
            .unwrap_or_else(|error| panic!("`{name}` satisfies `T: Sized`: {error:?}"))
            .expect("a specialization for the bounded call");
        assert_eq!(specialization.signature.parameters.as_ref(), &[abi], "`{name}` argument ABI");
        let [witness] = specialization.contract_witnesses.as_ref() else {
            panic!("one witness for the one bounded parameter: {:?}", specialization.contract_witnesses);
        };
        assert_eq!(
            index.kind(witness.concrete.node),
            Some(NodeKind::ImplBlock),
            "the `{name}` implementation block is the conformance's implementor"
        );
    }
}

#[test]
fn bounded_member_calls_dispatch_to_the_primitive_method_with_a_value_receiver() {
    for (name, argument, abi) in PRIMITIVES {
        let source = format!("{SIZED}i64 Main() {{ return SizeOf<{name}>({argument}); }}");
        let (db, _, unit, generation, index) = setup(&source);
        let outer = key(unit, generation, &index, NodeKind::CallExpression, 1);
        let specialization = generic_call_specialization(&db, outer).expect("bound proven").expect("specialization");
        let instance = generic_call_specialization_instance(&db, specialization)
            .expect("instance")
            .expect("materialized SizeOf instance");
        let member = key(unit, generation, &index, NodeKind::CallExpression, 0);
        let dispatched = generic_call_specialization_in_environment(&db, member, &instance)
            .unwrap_or_else(|error| panic!("`value.Size()` dispatches for `{name}`: {error:?}"))
            .expect("the bounded member call has a target");
        assert_eq!(dispatched.signature.parameters.as_ref(), &[abi], "`{name}` receiver is passed by value");
        assert_eq!(dispatched.signature.result, SemanticTypeId::I64);
        assert_eq!(method_receiver_abi_type(&db, dispatched.declaration), abi);
        let item = item_abi_signature(&db, dispatched.declaration)
            .expect("item signature query")
            .expect("a non-generic primitive implementation method has an item signature");
        assert_eq!(item, dispatched.signature, "the emitted item and the dispatched call agree on the `{name}` ABI");
    }
}

#[test]
fn a_primitive_without_an_implementation_does_not_satisfy_the_bound() {
    let source = format!("{SIZED}i64 Main() {{ return SizeOf<u16>(2_u16); }}");
    let (db, _, unit, generation, index) = setup(&source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let error = generic_call_specialization(&db, call).expect_err("u16 has no Sized implementation");
    let rendered = format!("{error:?}");
    assert!(rendered.contains("E1610") && rendered.contains("u16") && rendered.contains("Sized"), "{rendered}");
}

fn primitive_findings(source: &str) -> Vec<String> {
    let (db, _, unit, generation, _) = setup(source);
    let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
    unit_obligations(&db, root)
        .expect("unit obligations")
        .expect("root unit")
        .iter()
        .map(|obligation| match &obligation.kind {
            UnitObligationKind::InvalidPrimitiveImplementation { type_name, reason } => {
                format!("E1611 {type_name}: {reason}")
            }
            UnitObligationKind::ContractMethodMissingImplementation { method_name, .. } => {
                format!("E1601 {method_name}")
            }
            UnitObligationKind::ContractImplementationSignatureMismatch { method_name, .. } => {
                format!("E1602 {method_name}")
            }
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn malformed_primitive_implementations_are_e1611() {
    const CONTRACT: &str = "contract Sized { i64 Size(); } ";
    let cases = [
        (
            "impl pointer : Sized { pub i64 Size() { return 8; } }",
            "E1611 pointer: `pointer` cannot implement contracts",
        ),
        (
            "impl<T> i64 : Sized { pub i64 Size() { return 8; } }",
            "E1611 i64: an implementation for `i64` takes no type parameters",
        ),
        ("impl i64 { pub i64 Size() { return 8; } }", "E1611 i64: an implementation for `i64` must name a contract"),
        (
            "impl i64 : Sized { pub i64 Size() { return 8; } } impl i64 : Sized { pub i64 Size() { return 4; } }",
            "E1611 i64: `i64` already implements `Sized` in this source file",
        ),
        (
            "impl i64 : Sized { pub i64 Size(mut this) { return 8; } }",
            "E1611 i64: method `Size` of an implementation for `i64` cannot take `mut this`",
        ),
        ("impl i64 : Sized { }", "E1601 Size"),
        ("impl i64 : Sized { pub bool Size() { return true; } }", "E1602 Size"),
    ];
    for (implementation, expected) in cases {
        let findings = primitive_findings(&format!("{CONTRACT}{implementation}"));
        assert!(
            findings.iter().any(|finding| finding.starts_with(expected)),
            "`{implementation}` reports `{expected}`: {findings:?}"
        );
    }
    let accepted = primitive_findings(&format!("{CONTRACT}impl i64 : Sized {{ pub i64 Size() {{ return 8; }} }}"));
    assert!(accepted.is_empty(), "a well-formed primitive implementation has no finding: {accepted:?}");
}

/// Primitive implementations of one contract, with no generic function anywhere: every call in
/// `Main` is a direct call on a primitive value.
const SHOWN: &str = "contract Shown { string Show(); } \
    impl i32 : Shown { pub string Show() { return \"${this}\"; } } \
    impl i64 : Shown { pub string Show() { return \"${this}\"; } } \
    impl f64 : Shown { pub string Show() { return \"${this}\"; } } \
    impl bool : Shown { pub string Show() { return \"${this}\"; } } \
    impl string : Shown { pub string Show() { return this; } } ";

#[test]
fn primitive_values_call_their_implementation_methods_directly() {
    let source = format!(
        "{SHOWN}string Main(f64 x) {{ i64 n = 3; bool flag = true; string s = \"t\"; \
         return n.Show() + x.Show() + flag.Show() + s.Show() + 3.Show() + (n).Show() + \"lit\".Show(); }}"
    );
    let (db, _, unit, generation, index) = setup(&source);
    let expected = [
        ("n.Show()", SemanticTypeId::I64),
        ("x.Show()", SemanticTypeId::F64),
        ("flag.Show()", SemanticTypeId::BOOL),
        ("s.Show()", SemanticTypeId::STRING),
        ("3.Show()", SemanticTypeId::I32),
        ("(n).Show()", SemanticTypeId::I64),
        ("\"lit\".Show()", SemanticTypeId::STRING),
    ];
    for (occurrence, (spelling, abi)) in expected.into_iter().enumerate() {
        let call = key(unit, generation, &index, NodeKind::CallExpression, occurrence);
        let Ok(Some(beskid_queries::CallLowering::Direct(method))) = beskid_queries::call_lowering(&db, call) else {
            panic!("`{spelling}` is a direct call: {:?}", beskid_queries::call_lowering(&db, call));
        };
        assert_eq!(
            method_receiver_abi_type(&db, method),
            abi,
            "`{spelling}` calls the `{}` implementation",
            abi.as_str()
        );
        let arguments = beskid_queries::call_arguments(&db, call).expect("call arguments").expect("arguments");
        assert_eq!(arguments.len(), 1, "`{spelling}` passes its receiver as the only argument");
        let signature = beskid_queries::call_abi_signature(&db, call).expect("call signature").expect("signature");
        assert_eq!(signature.parameters.as_ref(), &[abi], "`{spelling}` receiver ABI");
        assert_eq!(signature.result, SemanticTypeId::STRING);
    }
}

#[test]
fn a_primitive_without_the_method_has_no_direct_call_target() {
    let source = format!("{SHOWN}string Main() {{ u16 n = 3_u16; return n.Show(); }}");
    let (db, _, unit, generation, index) = setup(&source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert!(
        !matches!(beskid_queries::call_lowering(&db, call), Ok(Some(beskid_queries::CallLowering::Direct(_)))),
        "u16 has no Shown implementation, so `n.Show()` resolves nothing"
    );
}

/// A `for` variable takes its iterable's element type and a `match` binding its payload type; both
/// call their primitive implementation directly, spelled as a path or as a member receiver.
#[test]
fn loop_variables_and_match_bindings_call_their_primitive_implementation_directly() {
    let source = format!(
        "{SHOWN}enum Reading {{ Count(i64 value), Ratio(f64 value), Flag(bool value), Empty }} \
         string Main(i64[] nums, f64[] ratios, Reading reading) {{ mut string out = \"\"; \
         for x in nums {{ out = out + x.Show(); }} \
         for r in ratios {{ out = out + (r).Show(); }} \
         string described = match reading {{ Reading::Count(v) => v.Show(), Reading::Ratio(v) => v.Show(), \
         Reading::Flag(v) => (v).Show(), Reading::Empty => \"none\", }}; \
         return out + described; }}"
    );
    let (db, _, unit, generation, index) = setup(&source);
    let expected = [
        ("x.Show() over i64[]", SemanticTypeId::I64),
        ("(r).Show() over f64[]", SemanticTypeId::F64),
        ("v.Show() on an i64 payload", SemanticTypeId::I64),
        ("v.Show() on an f64 payload", SemanticTypeId::F64),
        ("(v).Show() on a bool payload", SemanticTypeId::BOOL),
    ];
    for (occurrence, (spelling, abi)) in expected.into_iter().enumerate() {
        let call = key(unit, generation, &index, NodeKind::CallExpression, occurrence);
        let Ok(Some(beskid_queries::CallLowering::Direct(method))) = beskid_queries::call_lowering(&db, call) else {
            panic!("`{spelling}` is a direct call: {:?}", beskid_queries::call_lowering(&db, call));
        };
        assert_eq!(method_receiver_abi_type(&db, method), abi, "`{spelling}`");
        let arguments = beskid_queries::call_arguments(&db, call).expect("call arguments").expect("arguments");
        assert_eq!(arguments.len(), 1, "`{spelling}` passes its receiver as the only argument");
        let signature = beskid_queries::call_abi_signature(&db, call).expect("call signature").expect("signature");
        assert_eq!(signature.parameters.as_ref(), &[abi], "`{spelling}` receiver ABI");
    }
}
