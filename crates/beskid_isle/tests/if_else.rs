use std::path::PathBuf;

use beskid_isle::{AstNodeKey, FunctionEmitter, LiteralKind, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

struct IfElseFacts {
    nodes: [AstNodeKey; 6],
    has_else: bool,
    returns_values: bool,
}

struct ReturnSequenceFacts {
    nodes: [AstNodeKey; 3],
}

impl NodeFacts for ReturnSequenceFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let [block, first_return, second_return] = self.nodes;
        if key == block {
            Some(NodeKind::BlockExpression)
        } else if key == first_return || key == second_return {
            Some(NodeKind::ReturnStatement)
        } else {
            None
        }
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[0]).then_some(2)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.nodes[0])
            .then(|| self.nodes.get(usize::from(index) + 1).copied())
            .flatten()
    }

    fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }

    fn scalar_type(&self, _key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        None
    }
}

impl NodeFacts for IfElseFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let [
            if_node,
            condition,
            then_return,
            then_value,
            else_return,
            else_value,
        ] = self.nodes;
        if key == if_node {
            Some(NodeKind::IfStatement)
        } else if key == then_return || key == else_return {
            Some(NodeKind::ReturnStatement)
        } else if key == condition || key == then_value || key == else_value {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        let [_, condition, _, then_value, _, else_value] = self.nodes;
        if key == condition {
            Some(LiteralKind::Boolean)
        } else if key == then_value || key == else_value {
            Some(LiteralKind::Integer)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        let [
            if_node,
            condition,
            then_return,
            then_value,
            else_return,
            else_value,
        ] = self.nodes;
        match (key, index) {
            (key, 0) if key == if_node => Some(condition),
            (key, 1) if key == if_node => Some(then_return),
            (key, 2) if key == if_node && self.has_else => Some(else_return),
            (key, 0) if key == then_return && self.returns_values => Some(then_value),
            (key, 0) if key == else_return && self.returns_values => Some(else_value),
            _ => None,
        }
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        let [_, _, _, then_value, _, else_value] = self.nodes;
        if key == then_value {
            Some(1)
        } else if key == else_value {
            Some(2)
        } else {
            None
        }
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        (key == self.nodes[1]).then_some(true)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[1] {
            Some(types::I8)
        } else if key == self.nodes[3] || key == self.nodes[5] {
            Some(types::I32)
        } else {
            None
        }
    }
}

#[test]
fn if_else_rule_emits_verified_multi_block_clif() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/IfElse.bd"));
    let generation = SyntaxGenerationId(10);
    let facts = IfElseFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation,
            node: AstNodeId(index as u32 + 1),
        }),
        has_else: true,
        returns_values: true,
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_statement(
            UserFuncName::user(0, 13),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("verified if/else statement");

    let clif = function.display().to_string();
    assert!(clif.contains("brif"), "{clif}");
    assert!(clif.contains("iconst.i32 1"), "{clif}");
    assert!(clif.contains("iconst.i32 2"), "{clif}");
    assert_eq!(clif.matches("return").count(), 2, "{clif}");

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module
        .declare_function("if_else", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 1);
}

#[test]
fn return_after_if_without_else_reaches_merge() {
    // CYB-129: a reachable merge must not be sealed with `trap`; otherwise the
    // statement cursor treats the block as ended and drops the following `return`.
    // Shape mirrors runtime `if cond { side_effect }; return 0;`.
    struct IfThenReturnFacts {
        nodes: [AstNodeKey; 7],
    }
    impl NodeFacts for IfThenReturnFacts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            let [block, if_node, condition, then_stmt, then_value, ret, ret_value] = self.nodes;
            if key == block {
                Some(NodeKind::BlockExpression)
            } else if key == if_node {
                Some(NodeKind::IfStatement)
            } else if key == then_stmt {
                Some(NodeKind::ExpressionStatement)
            } else if key == ret {
                Some(NodeKind::ReturnStatement)
            } else if key == condition || key == then_value || key == ret_value {
                Some(NodeKind::LiteralExpression)
            } else {
                None
            }
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            let [_, _, condition, _, then_value, _, ret_value] = self.nodes;
            if key == condition {
                Some(LiteralKind::Boolean)
            } else if key == then_value || key == ret_value {
                Some(LiteralKind::Integer)
            } else {
                None
            }
        }

        fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
            (key == self.nodes[0]).then_some(2)
        }

        fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
            let [block, if_node, condition, then_stmt, then_value, ret, ret_value] = self.nodes;
            match (key, index) {
                (key, 0) if key == block => Some(if_node),
                (key, 1) if key == block => Some(ret),
                (key, 0) if key == if_node => Some(condition),
                (key, 1) if key == if_node => Some(then_stmt),
                (key, 0) if key == then_stmt => Some(then_value),
                (key, 0) if key == ret => Some(ret_value),
                _ => None,
            }
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            let [_, _, _, _, then_value, _, ret_value] = self.nodes;
            if key == then_value {
                Some(7)
            } else if key == ret_value {
                Some(0)
            } else {
                None
            }
        }

        fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
            (key == self.nodes[2]).then_some(false)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            if key == self.nodes[2] {
                Some(types::I8)
            } else if key == self.nodes[4] || key == self.nodes[6] {
                Some(types::I64)
            } else {
                None
            }
        }
    }

    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/IfThenReturn.bd"));
    let facts = IfThenReturnFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(13),
            node: AstNodeId(index as u32 + 1),
        }),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I64]);
    let function = emitter
        .emit_statement(
            UserFuncName::user(0, 16),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("return after if-without-else must lower");

    let clif = function.display().to_string();
    assert!(
        !clif.contains("trap"),
        "reachable merge must not trap:\n{clif}"
    );
    assert!(clif.contains("return"), "{clif}");

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module
        .declare_function("if_then_return", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 0, "fallthrough return must execute");
}

#[test]
fn if_without_else_terminates_reachable_fallthrough() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/IfWithoutElse.bd"));
    let facts = IfElseFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(11),
            node: AstNodeId(index as u32 + 1),
        }),
        has_else: false,
        returns_values: false,
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(
            UserFuncName::user(0, 14),
            emitter.signature([], []),
            &facts,
            facts.nodes[0],
        )
        .expect("verified if without else");

    assert!(function.display().to_string().contains("return"));
}

#[test]
fn statement_cursor_stops_after_a_terminator() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/ReturnSequence.bd"));
    let facts = ReturnSequenceFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(12),
            node: AstNodeId(index as u32 + 1),
        }),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(
            UserFuncName::user(0, 15),
            emitter.signature([], []),
            &facts,
            facts.nodes[0],
        )
        .expect("a statement sequence after return remains valid CLIF");

    assert_eq!(function.display().to_string().matches("return").count(), 1);
}
