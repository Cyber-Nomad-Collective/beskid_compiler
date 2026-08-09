use std::path::PathBuf;

use beskid_isle::syntax_types::LiteralKind;
use beskid_isle::{AstNodeKey, FunctionEmitter, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{Function, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

#[derive(Clone, Copy)]
enum Transfer {
    Break,
    Continue,
}

struct WhileFacts {
    nodes: [AstNodeKey; 6],
    condition: bool,
    transfer: Transfer,
}

impl NodeFacts for WhileFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::BlockExpression)
        } else if key == self.nodes[1] {
            Some(NodeKind::WhileStatement)
        } else if key == self.nodes[2] || key == self.nodes[5] {
            Some(NodeKind::LiteralExpression)
        } else if key == self.nodes[3] {
            Some(match self.transfer {
                Transfer::Break => NodeKind::BreakStatement,
                Transfer::Continue => NodeKind::ContinueStatement,
            })
        } else if key == self.nodes[4] {
            Some(NodeKind::ReturnStatement)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        if key == self.nodes[2] {
            Some(LiteralKind::Boolean)
        } else if key == self.nodes[5] {
            Some(LiteralKind::Integer)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            [self.nodes[1], self.nodes[4]].get(usize::from(index)).copied()
        } else if key == self.nodes[1] {
            [self.nodes[2], self.nodes[3]].get(usize::from(index)).copied()
        } else if key == self.nodes[4] && index == 0 {
            Some(self.nodes[5])
        } else {
            None
        }
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[0]).then_some(2)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[5]).then_some(9)
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        (key == self.nodes[2]).then_some(self.condition)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[2] {
            Some(types::I8)
        } else if key == self.nodes[5] {
            Some(types::I32)
        } else {
            None
        }
    }
}

fn emit_loop(
    isa: &dyn TargetIsa,
    transfer: Transfer,
    condition: bool,
    generation: u64,
    function_index: u32,
) -> Function {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/While.bd"));
    let facts = WhileFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(generation),
            node: AstNodeId(index as u32 + 1),
        }),
        condition,
        transfer,
    };
    let emitter = FunctionEmitter::new(isa);
    emitter
        .emit_statement(
            UserFuncName::user(0, function_index),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
        .expect("verified while statement")
}

fn execute(isa: std::sync::Arc<dyn TargetIsa>, name: &str, function: Function) -> i32 {
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let signature = function.signature.clone();
    let function_id = module.declare_function(name, Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    run()
}

#[test]
fn while_true_breaks_to_following_statement() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let function = emit_loop(isa.as_ref(), Transfer::Break, true, 12, 15);
    let clif = function.display().to_string();
    assert!(clif.contains("brif"), "{clif}");
    assert!(clif.matches("jump").count() >= 2, "{clif}");
    assert_eq!(execute(isa, "while_break", function), 9);
}

#[test]
fn while_false_contains_verified_continue_backedge() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let function = emit_loop(isa.as_ref(), Transfer::Continue, false, 13, 16);
    let clif = function.display().to_string();
    assert!(clif.contains("brif"), "{clif}");
    assert!(clif.matches("jump").count() >= 2, "{clif}");
    assert_eq!(execute(isa, "while_continue", function), 9);
}
