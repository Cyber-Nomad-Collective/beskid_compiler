use super::support::{
    AbiParam, AstNodeId, AstNodeKey, BeskidDatabase, CallConv, Function, FunctionBuilder, FunctionBuilderContext,
    InstBuilder, IsleContext, LiteralKind, LoweringErrorKind, NodeFacts, NodeKind, PathBuf, Signature, SourceUnitId,
    SyntaxGenerationId, Triple, lower_expression, settings, types, verify_function,
};

struct IntegerFacts {
    key: AstNodeKey,
}

impl NodeFacts for IntegerFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        (key == self.key).then_some(NodeKind::LiteralExpression)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.key).then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.key).then_some(42)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.key).then_some(types::I32)
    }
}

#[test]
fn integer_rule_emits_verified_stock_clif() {
    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(4),
    };
    let facts = IntegerFacts { key };
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let mut function = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature { params: vec![], returns: vec![AbiParam::new(types::I32)], call_conv: isa.default_call_conv() },
    );
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), key).expect("integer rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
}

#[test]
fn float_rule_emits_verified_stock_clif() {
    struct FloatFacts(AstNodeKey);
    impl NodeFacts for FloatFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            (key == self.0).then_some(NodeKind::LiteralExpression)
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.0).then_some(LiteralKind::Float)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
            (key == self.0).then_some(1.5)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.0).then_some(types::F64)
        }
    }

    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Float.bd")),
        generation: SyntaxGenerationId(5),
        node: AstNodeId(1),
    };
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let emitter = beskid_isle::FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(
            cranelift_codegen::ir::UserFuncName::user(0, 8),
            emitter.signature([], [types::F64]),
            &FloatFacts(key),
            key,
        )
        .expect("verified float rule");

    assert!(function.display().to_string().contains("f64const 0x1.8000000000000p0"));
}

#[test]
fn char_rule_emits_verified_stock_clif() {
    struct CharFacts(AstNodeKey);
    impl NodeFacts for CharFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            (key == self.0).then_some(NodeKind::LiteralExpression)
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.0).then_some(LiteralKind::Char)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn char_literal(&self, key: AstNodeKey) -> Option<char> {
            (key == self.0).then_some('ß')
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.0).then_some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Char.bd")),
        generation: SyntaxGenerationId(6),
        node: AstNodeId(1),
    };
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let emitter = beskid_isle::FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(
            cranelift_codegen::ir::UserFuncName::user(0, 9),
            emitter.signature([], [types::I32]),
            &CharFacts(key),
            key,
        )
        .expect("verified char rule");

    assert!(function.display().to_string().contains("iconst.i32 223"));
}

#[test]
fn missing_leaf_fact_is_a_keyed_lowering_error() {
    struct MissingFacts;
    impl NodeFacts for MissingFacts {
        fn node_kind(&self, _key: AstNodeKey) -> Option<NodeKind> {
            Some(NodeKind::LiteralExpression)
        }

        fn literal_kind(&self, _key: AstNodeKey) -> Option<LiteralKind> {
            Some(LiteralKind::Integer)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn scalar_type(&self, _key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            Some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(2),
        node: AstNodeId(8),
    };
    let mut function = Function::new();
    function.signature.call_conv = CallConv::Fast;
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
    let error = lower_expression(&mut IsleContext::new(&mut builder, &MissingFacts), key)
        .expect_err("missing fact must not fall back");

    assert_eq!(error.key(), key);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}
