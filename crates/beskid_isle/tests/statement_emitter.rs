use std::path::PathBuf;

use beskid_isle::syntax_types::LiteralKind;
use beskid_isle::{AstNodeKey, FunctionEmissionError, FunctionEmitter, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use target_lexicon::Triple;

struct ReturnFacts {
    statement: AstNodeKey,
    value: AstNodeKey,
}

impl NodeFacts for ReturnFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.statement {
            Some(NodeKind::ReturnStatement)
        } else if key == self.value {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.value).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.statement && index == 0).then_some(self.value)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.value).then_some(42)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.value).then_some(types::I32)
    }
}

#[test]
fn return_statement_recurses_through_isle_and_emits_verified_clif() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Return.bd"));
    let generation = SyntaxGenerationId(8);
    let facts = ReturnFacts {
        statement: AstNodeKey { unit, generation, node: AstNodeId(1) },
        value: AstNodeKey { unit, generation, node: AstNodeId(2) },
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 11), emitter.signature([], [types::I32]), &facts, facts.statement)
        .expect("verified return statement");

    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn unterminated_statement_is_rejected_by_mandatory_verification() {
    struct ExpressionFacts(ReturnFacts);
    impl NodeFacts for ExpressionFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.0.statement {
                Some(NodeKind::ExpressionStatement)
            } else if key == self.0.value {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.0.value).then_some(LiteralKind::Integer)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.0.statement && index == 0).then_some(self.0.value)
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            (key == self.0.value).then_some(7)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.0.value).then_some(types::I32)
        }
    }

    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Expression.bd"));
    let generation = SyntaxGenerationId(9);
    let facts = ExpressionFacts(ReturnFacts {
        statement: AstNodeKey { unit, generation, node: AstNodeId(1) },
        value: AstNodeKey { unit, generation, node: AstNodeId(2) },
    });
    let emitter = FunctionEmitter::new(isa.as_ref());
    // Unit signatures receive an implicit empty `return`; non-unit signatures must still
    // terminate explicitly or the emitter rejects the body before verification.
    let error = emitter
        .emit_statement(UserFuncName::user(0, 12), emitter.signature([], [types::I32]), &facts, facts.0.statement)
        .expect_err("unterminated non-unit CLIF must not escape the emitter");

    assert!(matches!(error, FunctionEmissionError::Verification { .. }));
}
