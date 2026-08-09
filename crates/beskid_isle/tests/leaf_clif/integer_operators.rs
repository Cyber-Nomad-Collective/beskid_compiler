use super::support::{
    AstNodeId, AstNodeKey, BeskidDatabase, Function, FunctionBuilder, FunctionBuilderContext, InstBuilder, IsleContext,
    LiteralKind, NodeFacts, NodeKind, OperatorFact, PathBuf, SourceUnitId, SyntaxGenerationId, lower_expression, types,
};

#[test]
fn binary_rule_recurses_through_ast_keys_and_emits_iadd() {
    struct BinaryFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
    }
    impl NodeFacts for BinaryFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.root {
                Some(NodeKind::BinaryExpression)
            } else if key == self.left || key == self.right {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.left || key == self.right).then_some(LiteralKind::Integer)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::Add)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key == self.root, index) {
                (true, 0) => Some(self.left),
                (true, 1) => Some(self.right),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            if key == self.left {
                Some(20)
            } else if key == self.right {
                Some(22)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd"));
    let generation = SyntaxGenerationId(3);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let facts = BinaryFacts { root: node(1), left: node(2), right: node(3) };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("binary rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("iadd"), "{clif}");
    assert!(clif.contains("iconst.i32 20"), "{clif}");
    assert!(clif.contains("iconst.i32 22"), "{clif}");
}

#[test]
fn integer_bitwise_and_rule_emits_band() {
    struct BitwiseAndFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
    }
    impl NodeFacts for BitwiseAndFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.root {
                Some(NodeKind::BinaryExpression)
            } else if key == self.left || key == self.right {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.left || key == self.right).then_some(LiteralKind::Integer)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::BitAnd)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key == self.root, index) {
                (true, 0) => Some(self.left),
                (true, 1) => Some(self.right),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            if key == self.left {
                Some(47)
            } else if key == self.right {
                Some(31)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd"));
    let generation = SyntaxGenerationId(3);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let facts = BitwiseAndFacts { root: node(1), left: node(2), right: node(3) };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value =
            lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("bitwise AND rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("band"), "{clif}");
}

#[test]
fn integer_shift_and_or_rules_emit_stock_clif() {
    struct ShiftFacts {
        root: AstNodeKey,
        shift: AstNodeKey,
        generation: AstNodeKey,
        amount: AstNodeKey,
        slot: AstNodeKey,
    }

    impl NodeFacts for ShiftFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            (key == self.root || key == self.shift).then_some(NodeKind::BinaryExpression).or_else(|| {
                (key == self.generation || key == self.amount || key == self.slot)
                    .then_some(NodeKind::LiteralExpression)
            })
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.generation || key == self.amount || key == self.slot).then_some(LiteralKind::Integer)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            if key == self.root {
                Some(OperatorFact::BitOr)
            } else if key == self.shift {
                Some(OperatorFact::Shl)
            } else {
                None
            }
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key, index) {
                (key, 0) if key == self.root => Some(self.shift),
                (key, 1) if key == self.root => Some(self.slot),
                (key, 0) if key == self.shift => Some(self.generation),
                (key, 1) if key == self.shift => Some(self.amount),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            if key == self.generation {
                Some(7)
            } else if key == self.amount {
                Some(32)
            } else if key == self.slot {
                Some(3)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.shift || key == self.generation || key == self.amount || key == self.slot)
                .then_some(types::I64)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Shift.bd"));
    let generation = SyntaxGenerationId(3);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let facts = ShiftFacts { root: node(1), shift: node(2), generation: node(3), amount: node(4), slot: node(5) };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value =
            lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("shift and OR rules");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("ishl"), "{clif}");
    assert!(clif.contains("bor"), "{clif}");
}

#[test]
fn integer_logical_right_shift_rule_emits_ushr() {
    struct ShiftFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
    }

    impl NodeFacts for ShiftFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.root {
                Some(NodeKind::BinaryExpression)
            } else if key == self.left || key == self.right {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }
        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.left || key == self.right).then_some(LiteralKind::Integer)
        }
        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::Shr)
        }
        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key == self.root, index) {
                (true, 0) => Some(self.left),
                (true, 1) => Some(self.right),
                _ => None,
            }
        }
        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            if key == self.left {
                Some(-1)
            } else if key == self.right {
                Some(1)
            } else {
                None
            }
        }
        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/ShiftRight.bd"));
    let node = |id| AstNodeKey { unit, generation: SyntaxGenerationId(3), node: AstNodeId(id) };
    let facts = ShiftFacts { root: node(1), left: node(2), right: node(3) };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root)
            .expect("logical right shift rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }
    assert!(function.display().to_string().contains("ushr"));
}
