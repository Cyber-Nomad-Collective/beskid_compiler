use std::path::PathBuf;

use beskid_isle::{AstNodeKey, ISLE_INPUTS, NodeKind, RULE_COUNT, Value, generated};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};

#[test]
fn generated_isle_has_real_rules_and_a_context() {
    fn requires_context<T: generated::Context>() {}

    struct EmptyContext;
    impl generated::Context for EmptyContext {
        fn node_kind(&mut self, _key: AstNodeKey) -> Option<NodeKind> {
            None
        }

        fn emit_integer(&mut self, _key: AstNodeKey) -> Value {
            Value::from_u32(0)
        }
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

        fn emit_integer(&mut self, _key: AstNodeKey) -> Value {
            Value::from_u32(7)
        }
    }

    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(3),
    };

    assert_eq!(
        generated::constructor_lower_expression(&mut Context(NodeKind::Unsupported), key),
        None
    );
    assert_eq!(
        generated::constructor_lower_expression(&mut Context(NodeKind::IntegerLiteral), key),
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
