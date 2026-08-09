use super::support::{
    AbiParam, AstNodeId, AstNodeKey, BeskidDatabase, Function, FunctionBuilder, FunctionBuilderContext, InstBuilder,
    IsleContext, JITBuilder, JITModule, Linkage, LiteralKind, Module, NodeFacts, NodeKind, OperatorFact, PathBuf,
    Signature, SourceUnitId, SyntaxGenerationId, Triple, default_libcall_names, lower_expression, settings, types,
    verify_function,
};

#[test]
fn grouped_expression_unwraps_child_and_emits_verified_stock_clif() {
    struct GroupedFacts {
        group: AstNodeKey,
        inner: AstNodeKey,
    }
    impl NodeFacts for GroupedFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.group {
                Some(NodeKind::GroupedExpression)
            } else if key == self.inner {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.inner).then_some(LiteralKind::Integer)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.group && index == 0).then_some(self.inner)
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            (key == self.inner).then_some(42)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.group || key == self.inner).then_some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd"));
    let generation = SyntaxGenerationId(5);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let facts = GroupedFacts { group: node(1), inner: node(2) };
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
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.group)
            .expect("grouped expression rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
}

#[test]
fn boolean_not_executes_with_canonical_zero_or_one_result() {
    struct NotFacts {
        root: AstNodeKey,
        value: AstNodeKey,
    }
    impl NodeFacts for NotFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.root {
                Some(NodeKind::UnaryExpression)
            } else if key == self.value {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.value).then_some(LiteralKind::Boolean)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::Not)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.root && index == 0).then_some(self.value)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
            (key == self.value).then_some(true)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.value).then_some(types::I8)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd"));
    let node = |id| AstNodeKey { unit, generation: SyntaxGenerationId(4), node: AstNodeId(id) };
    let facts = NotFacts { root: node(1), value: node(2) };
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("JIT"));
    let emitter = beskid_isle::FunctionEmitter::new(module.isa());
    let signature = emitter.signature([], [types::I8]);
    let function = emitter
        .emit_expression(cranelift_codegen::ir::UserFuncName::user(0, 7), signature.clone(), &facts, facts.root)
        .expect("verified bool not");
    let function_id = module.declare_function("bool_not", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> u8 = unsafe { std::mem::transmute(code) };

    // `!` is bool-only, so `!true` must be the canonical `false` (0). Flipping every bit would
    // yield 254, which is still truthy and compares unequal to `false`.
    assert_eq!(run(), 0);
}

#[test]
fn binary_float_add_emits_fadd() {
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
            (key == self.left || key == self.right).then_some(LiteralKind::Float)
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

        fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
            if key == self.left {
                Some(1.5)
            } else if key == self.right {
                Some(2.25)
            } else {
                None
            }
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::F64)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/FloatAdd.bd"));
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
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("float add rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("fadd"), "{clif}");
    assert!(!clif.contains("iadd"), "{clif}");
}

#[test]
fn binary_u8_less_than_emits_unsigned_compare() {
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
            (key == self.root).then_some(OperatorFact::Lt)
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
                Some(200)
            } else if key == self.right {
                Some(10)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            if key == self.root || key == self.left || key == self.right { Some(types::I8) } else { None }
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/U8Lt.bd"));
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
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root).expect("u8 lt rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult") || clif.contains("ult"), "expected unsigned less-than for u8:\n{clif}");
    assert!(!clif.contains("icmp slt") && !clif.contains(" slt "), "must not use signed less-than for u8:\n{clif}");
}

#[test]
fn sdiv_traps_on_zero_divisor() {
    struct DivFacts {
        root: AstNodeKey,
        left: AstNodeKey,
        right: AstNodeKey,
    }
    impl NodeFacts for DivFacts {
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
            (key == self.root).then_some(OperatorFact::Div)
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
                Some(10)
            } else if key == self.right {
                Some(0)
            } else {
                None
            }
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.left || key == self.right).then_some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/DivZero.bd"));
    let generation = SyntaxGenerationId(7);
    let node = |id| AstNodeKey { unit, generation, node: AstNodeId(id) };
    let facts = DivFacts { root: node(1), left: node(2), right: node(3) };
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let emitter = beskid_isle::FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(
            cranelift_codegen::ir::UserFuncName::user(0, 10),
            emitter.signature([], [types::I32]),
            &facts,
            facts.root,
        )
        .expect("verified sdiv trapz");

    let clif = function.display().to_string();
    assert!(clif.contains("trapnz"), "expected trapnz:\n{clif}");
    assert!(clif.contains("int_divz"), "expected IntegerDivisionByZero trap code:\n{clif}");
}

#[test]
fn logical_not_emits_zero_compare_instead_of_bitwise_not() {
    struct NotFacts {
        root: AstNodeKey,
        value: AstNodeKey,
    }
    impl NodeFacts for NotFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            if key == self.root {
                Some(NodeKind::UnaryExpression)
            } else if key == self.value {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.value).then_some(LiteralKind::Boolean)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::Not)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.root && index == 0).then_some(self.value)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            None
        }

        fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
            (key == self.value).then_some(false)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.value).then_some(types::I8)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Not.bd"));
    let node = |id| AstNodeKey { unit, generation: SyntaxGenerationId(5), node: AstNodeId(id) };
    let facts = NotFacts { root: node(1), value: node(2) };
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("JIT"));
    let emitter = beskid_isle::FunctionEmitter::new(module.isa());
    let signature = emitter.signature([], [types::I8]);
    let function = emitter
        .emit_expression(cranelift_codegen::ir::UserFuncName::user(0, 8), signature.clone(), &facts, facts.root)
        .expect("verified logical not");

    let clif = function.display().to_string();
    assert!(clif.contains("icmp"), "expected a compare against zero in CLIF:\n{clif}");
    assert!(!clif.contains("bxor"), "expected NO bxor (bitwise NOT) in CLIF:\n{clif}");
    assert!(!clif.contains("-1"), "expected NO all-ones constant in CLIF:\n{clif}");

    let function_id = module.declare_function("logical_not", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> u8 = unsafe { std::mem::transmute(code) };

    assert_eq!(run(), 1, "!false must be the canonical true (1)");
}
