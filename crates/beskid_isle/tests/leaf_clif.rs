use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, IsleContext, LiteralKind, NodeFacts, NodeKind, OperatorFact, lower_expression,
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
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let mut function = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature {
            params: vec![],
            returns: vec![AbiParam::new(types::I32)],
            call_conv: isa.default_call_conv(),
        },
    );
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), key)
            .expect("integer rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
    assert!(
        !clif.contains("SystemV"),
        "target call convention is ISA-derived"
    );
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
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let emitter = beskid_isle::FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(
            cranelift_codegen::ir::UserFuncName::user(0, 8),
            emitter.signature([], [types::F64]),
            &FloatFacts(key),
            key,
        )
        .expect("verified float rule");

    assert!(
        function
            .display()
            .to_string()
            .contains("f64const 0x1.8000000000000p0")
    );
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
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
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
    let node = |id| AstNodeKey {
        unit,
        generation,
        node: AstNodeId(id),
    };
    let facts = BinaryFacts {
        root: node(1),
        left: node(2),
        right: node(3),
    };
    let mut function = Function::new();
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let value = lower_expression(&mut IsleContext::new(&mut builder, &facts), facts.root)
            .expect("binary rule");
        builder.ins().return_(&[value]);
        builder.finalize();
    }

    let clif = function.display().to_string();
    assert!(clif.contains("iadd"), "{clif}");
    assert!(clif.contains("iconst.i32 20"), "{clif}");
    assert!(clif.contains("iconst.i32 22"), "{clif}");
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
    let node = |id| AstNodeKey {
        unit,
        generation: SyntaxGenerationId(4),
        node: AstNodeId(id),
    };
    let facts = NotFacts {
        root: node(1),
        value: node(2),
    };
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("JIT"));
    let emitter = beskid_isle::FunctionEmitter::new(module.isa());
    let signature = emitter.signature([], [types::I8]);
    let function = emitter
        .emit_expression(
            cranelift_codegen::ir::UserFuncName::user(0, 7),
            signature.clone(),
            &facts,
            facts.root,
        )
        .expect("verified bool not");
    let function_id = module
        .declare_function("bool_not", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> u8 = unsafe { std::mem::transmute(code) };

    assert_eq!(run(), 0);
}
