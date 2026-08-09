use super::support::{
    AstNodeId, AstNodeKey, BeskidDatabase, EnumLayout, EnumVariantLayout, FieldLayout, Function, FunctionBuilder,
    FunctionBuilderContext, InstBuilder, IsleContext, LiteralKind, NodeFacts, NodeKind, OperatorFact, PathBuf,
    SourceUnitId, SyntaxGenerationId, lower_expression, types,
};

/// Verifies that `EnumEq` lowers to discriminant load + compare on enum pointer values.
/// Enum equality operators (EnumEq/EnumNotEq) route through `clif_enum_eq`/`clif_enum_ne`
/// which load the tag from each operand at the layout-specified offset and compare them.
#[test]
fn enum_equality_compares_discriminant_tags_primary() {
    struct EnumEqFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
        layout: EnumLayout,
    }
    impl NodeFacts for EnumEqFacts {
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
            (key == self.root).then_some(OperatorFact::EnumEq)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key == self.root, index) {
                (true, 0) => Some(self.left),
                (true, 1) => Some(self.right),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            // Return pointer-sized integer values that will serve as enum object pointers.
            if key == self.left {
                Some(0x1000)
            } else if key == self.right {
                Some(0x2000)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }

        fn binary_enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
            (key == self.root).then(|| self.layout.clone())
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/EnumEq.bd"));
    let generation = SyntaxGenerationId(20);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    // Enum with i32 tag at offset 0, two variants (0: no payload, 1: i32 payload at offset 4).
    let layout = EnumLayout::new(
        8,
        3,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, None), EnumVariantLayout::new(1, Some(FieldLayout::new(types::I32, 4)))],
    );
    let facts = EnumEqFacts { root: node(1), left: node(2), right: node(3), layout };

    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("enum eq rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "must load discriminant: {clif}");
    assert!(clif.contains("icmp"), "must compare discriminants: {clif}");
    assert!(clif.contains("eq"), "must emit equality comparison: {clif}");
}

/// Verifies `EnumNotEq` lowers to a negated discriminant comparison.
#[test]
fn enum_not_equality_compares_discriminant_tags_negated_primary() {
    struct EnumNeFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
        layout: EnumLayout,
    }
    impl NodeFacts for EnumNeFacts {
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
            (key == self.root).then_some(OperatorFact::EnumNotEq)
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
                Some(0x3000)
            } else if key == self.right {
                Some(0x4000)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }

        fn binary_enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
            (key == self.root).then(|| self.layout.clone())
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/EnumNe.bd"));
    let generation = SyntaxGenerationId(21);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let layout = EnumLayout::new(
        8,
        3,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, None), EnumVariantLayout::new(1, Some(FieldLayout::new(types::I32, 4)))],
    );
    let facts = EnumNeFacts { root: node(1), left: node(2), right: node(3), layout };

    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("enum ne rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "must load discriminant: {clif}");
    assert!(clif.contains("icmp"), "must compare discriminants: {clif}");
    assert!(clif.contains("ne"), "must emit inequality comparison: {clif}");
}

/// Verifies that `EnumEq` lowers to discriminant load + compare on enum pointer values.
/// Enum equality operators (EnumEq/EnumNotEq) route through `clif_enum_eq`/`clif_enum_ne`
/// which load the tag from each operand at the layout-specified offset and compare them.
#[test]
fn enum_equality_compares_discriminant_tags() {
    struct EnumEqFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
        layout: EnumLayout,
    }
    impl NodeFacts for EnumEqFacts {
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
            (key == self.root).then_some(OperatorFact::EnumEq)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            match (key == self.root, index) {
                (true, 0) => Some(self.left),
                (true, 1) => Some(self.right),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            // Return pointer-sized integer values that will serve as enum object pointers.
            if key == self.left {
                Some(0x1000)
            } else if key == self.right {
                Some(0x2000)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }

        fn binary_enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
            (key == self.root).then(|| self.layout.clone())
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/EnumEq.bd"));
    let generation = SyntaxGenerationId(20);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    // Enum with i32 tag at offset 0, two variants (0: no payload, 1: i32 payload at offset 4).
    let layout = EnumLayout::new(
        8,
        3,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, None), EnumVariantLayout::new(1, Some(FieldLayout::new(types::I32, 4)))],
    );
    let facts = EnumEqFacts { root: node(1), left: node(2), right: node(3), layout };

    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("enum eq rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "must load discriminant: {clif}");
    assert!(clif.contains("icmp"), "must compare discriminants: {clif}");
    assert!(clif.contains("eq"), "must emit equality comparison: {clif}");
}

/// Verifies `EnumNotEq` lowers to a negated discriminant comparison.
#[test]
fn enum_not_equality_compares_discriminant_tags_negated() {
    struct EnumNeFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
        layout: EnumLayout,
    }
    impl NodeFacts for EnumNeFacts {
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
            (key == self.root).then_some(OperatorFact::EnumNotEq)
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
                Some(0x3000)
            } else if key == self.right {
                Some(0x4000)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I64)
        }

        fn binary_enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
            (key == self.root).then(|| self.layout.clone())
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/EnumNe.bd"));
    let generation = SyntaxGenerationId(21);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let layout = EnumLayout::new(
        8,
        3,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, None), EnumVariantLayout::new(1, Some(FieldLayout::new(types::I32, 4)))],
    );
    let facts = EnumNeFacts { root: node(1), left: node(2), right: node(3), layout };

    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("enum ne rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "must load discriminant: {clif}");
    assert!(clif.contains("icmp"), "must compare discriminants: {clif}");
    assert!(clif.contains("ne"), "must emit inequality comparison: {clif}");
}
