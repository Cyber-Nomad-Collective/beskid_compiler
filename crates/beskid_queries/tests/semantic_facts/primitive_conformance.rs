//! Primitive types conform to contracts through `impl <primitive> : Contract` blocks: the bound is
//! proven, the bounded member call dispatches to the primitive's own method with the receiver
//! passed by value, and malformed primitive implementations are E1611.

use super::support::{key, setup};
use beskid_analysis::syntax::AstNodeId;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, SemanticTypeId, UnitObligationKind, generic_call_specialization, generic_call_specialization_in_environment,
    generic_call_specialization_instance, item_abi_signature, method_receiver_abi_type, unit_obligations,
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
            UnitObligationKind::ContractMethodMissingImplementation { method_name, .. } => format!("E1601 {method_name}"),
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
        ("impl pointer : Sized { pub i64 Size() { return 8; } }", "E1611 pointer: `pointer` cannot implement contracts"),
        ("impl<T> i64 : Sized { pub i64 Size() { return 8; } }", "E1611 i64: an implementation for `i64` takes no type parameters"),
        ("impl i64 { pub i64 Size() { return 8; } }", "E1611 i64: an implementation for `i64` must name a contract"),
        (
            "impl i64 : Sized { pub i64 Size() { return 8; } } impl i64 : Sized { pub i64 Size() { return 4; } }",
            "E1611 i64: `i64` already implements `Sized` in this source file",
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
