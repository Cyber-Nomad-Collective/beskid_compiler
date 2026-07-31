use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, EnumLayout, EnumVariantLayout, FieldLayout, IsleContext, LiteralKind, LoweringErrorKind, NodeFacts,
    NodeKind, OperatorFact, lower_expression, lower_statement,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

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

    // Bitwise NOT on I8: !1 = 1 xor -1 = 254
    assert_eq!(run(), 254);
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
fn bitwise_not_emits_bxor_with_all_ones() {
    struct BnotFacts {
        root: AstNodeKey,
        value: AstNodeKey,
    }
    impl NodeFacts for BnotFacts {
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
            (key == self.value).then_some(LiteralKind::Integer)
        }

        fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
            (key == self.root).then_some(OperatorFact::Not)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            (key == self.root && index == 0).then_some(self.value)
        }

        fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
            Some(42)
        }

        fn boolean_literal(&self, _key: AstNodeKey) -> Option<bool> {
            None
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.root || key == self.value).then_some(types::I32)
        }
    }

    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Bnot.bd"));
    let node = |id| AstNodeKey { unit, generation: SyntaxGenerationId(5), node: AstNodeId(id) };
    let facts = BnotFacts { root: node(1), value: node(2) };
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("JIT"));
    let emitter = beskid_isle::FunctionEmitter::new(module.isa());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(cranelift_codegen::ir::UserFuncName::user(0, 8), signature.clone(), &facts, facts.root)
        .expect("verified bitwise not");

    let clif = function.display().to_string();
    assert!(clif.contains("bxor"), "expected bxor (bitwise XOR) in CLIF:\n{clif}");
    assert!(clif.contains("iconst.i32 -1"), "expected iconst.i32 -1 (all-ones) in CLIF:\n{clif}");
    assert!(!clif.contains("icmp"), "expected NO icmp (boolean compare) in CLIF:\n{clif}");

    let function_id = module.declare_function("bitwise_not", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };

    assert_eq!(run(), !42i32);
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
