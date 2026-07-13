use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, CallKind, ISLE_INPUTS, LiteralKind, NodeKind, OperatorFact, RULE_COUNT, Value,
    generated,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};

macro_rules! passthrough_clif_methods {
    () => {
        fn clif_iadd(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_isub(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_imul(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_sdiv(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_srem(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_eq(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_ne(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_slt(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_sle(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_sgt(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_sge(&mut self, left: Value, _right: Value) -> Value {
            left
        }
        fn clif_short_circuit_or(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(0))
        }
        fn clif_short_circuit_and(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(0))
        }
        fn clif_ineg(&mut self, value: Value) -> Value {
            value
        }
        fn clif_bnot(&mut self, value: Value) -> Value {
            value
        }
        fn emit_direct_call(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }
        fn emit_runtime_intrinsic(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }
        fn discard_value(&mut self, _value: Value) {}
        fn emit_return(&mut self, _key: AstNodeKey) -> Option<()> {
            Some(())
        }
        fn emit_local_read(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }
    };
}

#[test]
fn generated_isle_has_real_rules_and_a_context() {
    fn requires_context<T: generated::Context>() {}

    struct EmptyContext;
    impl generated::Context for EmptyContext {
        fn node_kind(&mut self, _key: AstNodeKey) -> Option<NodeKind> {
            None
        }

        fn literal_kind(&mut self, _key: AstNodeKey) -> Option<LiteralKind> {
            None
        }

        fn operator_fact(&mut self, _key: AstNodeKey) -> Option<OperatorFact> {
            None
        }

        fn call_kind(&mut self, _key: AstNodeKey) -> Option<CallKind> {
            None
        }

        fn child_at(&mut self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
            None
        }

        fn emit_integer(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(0))
        }

        fn emit_boolean(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(0))
        }

        passthrough_clif_methods!();
    }

    requires_context::<EmptyContext>();
    assert!(
        std::hint::black_box(RULE_COUNT) > 0,
        "the lowering selector must contain real rules"
    );
}

#[test]
fn partial_expression_constructor_returns_none_when_no_rule_matches() {
    struct Context(NodeKind);
    impl generated::Context for Context {
        fn node_kind(&mut self, _key: AstNodeKey) -> Option<NodeKind> {
            Some(self.0)
        }

        fn literal_kind(&mut self, _key: AstNodeKey) -> Option<LiteralKind> {
            Some(LiteralKind::Integer)
        }

        fn operator_fact(&mut self, _key: AstNodeKey) -> Option<OperatorFact> {
            None
        }

        fn call_kind(&mut self, _key: AstNodeKey) -> Option<CallKind> {
            None
        }

        fn child_at(&mut self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
            None
        }

        fn emit_integer(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(7))
        }

        fn emit_boolean(&mut self, _key: AstNodeKey) -> Option<Value> {
            Some(Value::from_u32(7))
        }

        passthrough_clif_methods!();
    }

    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(3),
    };

    assert_eq!(
        generated::constructor_lower_expression(&mut Context(NodeKind::Program), key),
        None
    );
    assert_eq!(
        generated::constructor_lower_expression(&mut Context(NodeKind::LiteralExpression), key),
        Some(Value::from_u32(7))
    );
}

#[test]
fn isle_inputs_are_in_one_stable_order() {
    assert_eq!(
        ISLE_INPUTS,
        &[
            "types.isle",
            "ast.isle",
            "expressions.isle",
            "literals.isle",
            "binary.isle",
            "unary_casts.isle",
            "calls.isle",
            "statements.isle",
            "control_flow.isle",
            "memory.isle",
            "runtime_intrinsics.isle",
            "items.isle",
        ]
    );
}
