use super::support::{
    AstNodeId, AstNodeKey, BeskidDatabase, Function, FunctionBuilder, FunctionBuilderContext, IsleContext,
    LoweringErrorKind, NodeFacts, NodeKind, PathBuf, SourceUnitId, SyntaxGenerationId, lower_statement,
};

#[test]
fn block_cursor_reports_unsupported_child_instead_of_enclosing_block() {
    struct BlockFacts {
        block: AstNodeKey,
        unsupported: AstNodeKey,
    }

    impl NodeFacts for BlockFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.block {
                Some(NodeKind::BlockExpression)
            } else if key == self.unsupported {
                // Spawn expressions are expression-only and therefore deliberately
                // have no statement lowering rule.
                Some(NodeKind::SpawnExpression)
            } else {
                None
            }
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.block && index == 0).then_some(self.unsupported)
        }

        fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
            (key == self.block).then_some(1)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn scalar_type(&self, _key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            None
        }
    }

    let db = BeskidDatabase::default();
    let block = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Attribution.bd")),
        generation: SyntaxGenerationId(9),
        node: AstNodeId(1),
    };
    let unsupported = AstNodeKey { node: AstNodeId(2), ..block };
    let facts = BlockFacts { block, unsupported };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let error = lower_statement(&mut IsleContext::new(&mut builder, &facts), block)
        .expect_err("unsupported child must fail closed");

    assert_eq!(error.key(), unsupported);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}

#[test]
fn if_statement_reports_unsupported_condition_instead_of_enclosing_if() {
    struct IfFacts {
        if_node: AstNodeKey,
        unsupported_condition: AstNodeKey,
        then_statement: AstNodeKey,
    }

    impl NodeFacts for IfFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.if_node {
                Some(NodeKind::IfStatement)
            } else if key == self.unsupported_condition {
                Some(NodeKind::SpawnExpression)
            } else if key == self.then_statement {
                Some(NodeKind::ReturnStatement)
            } else {
                None
            }
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key, index) {
                (key, 0) if key == self.if_node => Some(self.unsupported_condition),
                (key, 1) if key == self.if_node => Some(self.then_statement),
                _ => None,
            }
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn scalar_type(&self, _key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            None
        }
    }

    let db = BeskidDatabase::default();
    let if_node = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(10),
        node: AstNodeId(1),
    };
    let unsupported_condition = AstNodeKey { node: AstNodeId(2), ..if_node };
    let then_statement = AstNodeKey { node: AstNodeId(3), ..if_node };
    let facts = IfFacts { if_node, unsupported_condition, then_statement };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let error = lower_statement(&mut IsleContext::new(&mut builder, &facts), if_node)
        .expect_err("unsupported condition must fail closed");

    assert_eq!(error.key(), unsupported_condition);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}
