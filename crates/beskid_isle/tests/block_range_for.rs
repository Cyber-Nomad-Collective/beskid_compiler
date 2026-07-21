use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, FunctionEmissionError, FunctionEmitter, LiteralKind, LocalSlotId, LoweringErrorKind,
    NodeFacts, NodeKind, OperatorFact, RangeFact,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{Type, UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

struct BlockFacts {
    nodes: [AstNodeKey; 4],
    block_type: Type,
}

impl NodeFacts for BlockFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::BlockExpression)
        } else if key == self.nodes[1] {
            Some(NodeKind::LetStatement)
        } else if key == self.nodes[2] {
            Some(NodeKind::LiteralExpression)
        } else if key == self.nodes[3] {
            Some(NodeKind::PathExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.nodes[2]).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.nodes[0] && index == 0).then_some(self.nodes[1])
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[0]).then_some(1)
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[1]).then_some(self.nodes[2])
    }

    fn block_result(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[0]).then_some(self.nodes[3])
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[2]).then_some(41)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        if key == self.nodes[0] {
            Some(self.block_type)
        } else if self.nodes[1..].contains(&key) {
            Some(types::I32)
        } else {
            None
        }
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.nodes[1] || key == self.nodes[3]).then_some(LocalSlotId {
            owner_node: 0,
            index: 0,
        })
    }
}

fn keys<const N: usize>(path: &str, generation: u64) -> [AstNodeKey; N] {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from(path));
    std::array::from_fn(|index| AstNodeKey {
        unit,
        generation: SyntaxGenerationId(generation),
        node: AstNodeId(index as u32 + 1),
    })
}

#[test]
fn block_expression_sequences_statements_and_returns_typed_tail() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = BlockFacts {
        nodes: keys("/tmp/BlockValue.bd", 19),
        block_type: types::I32,
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(
            UserFuncName::user(0, 32),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("verified block expression");
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module
        .declare_function("block_value", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 41);
}

#[test]
fn mismatched_block_tail_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = BlockFacts {
        nodes: keys("/tmp/BlockError.bd", 20),
        block_type: types::I64,
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(
            UserFuncName::user(0, 33),
            emitter.signature([], [types::I64]),
            &facts,
            facts.nodes[0],
        )
        .expect_err("semantic block type must equal tail type");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidBlockExpression);
}

struct RangeForFacts {
    nodes: [AstNodeKey; 16],
    start: i64,
    end: i64,
    step: i64,
    inclusive: bool,
}

impl NodeFacts for RangeForFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let kind = if key == self.nodes[0] || key == self.nodes[7] {
            NodeKind::BlockExpression
        } else if key == self.nodes[1] {
            NodeKind::LetStatement
        } else if key == self.nodes[3] {
            NodeKind::ForStatement
        } else if key == self.nodes[4] {
            NodeKind::RangeExpression
        } else if [self.nodes[2], self.nodes[5], self.nodes[6]].contains(&key) {
            NodeKind::LiteralExpression
        } else if key == self.nodes[8] {
            NodeKind::ExpressionStatement
        } else if key == self.nodes[9] {
            NodeKind::AssignExpression
        } else if key == self.nodes[11] {
            NodeKind::BinaryExpression
        } else if [
            self.nodes[10],
            self.nodes[12],
            self.nodes[13],
            self.nodes[15],
        ]
        .contains(&key)
        {
            NodeKind::PathExpression
        } else if key == self.nodes[14] {
            NodeKind::ReturnStatement
        } else {
            return None;
        };
        Some(kind)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        [self.nodes[2], self.nodes[5], self.nodes[6]]
            .contains(&key)
            .then_some(LiteralKind::Integer)
    }

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        (key == self.nodes[11]).then_some(OperatorFact::Add)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        let children = if key == self.nodes[0] {
            vec![self.nodes[1], self.nodes[3], self.nodes[14]]
        } else if key == self.nodes[3] {
            vec![self.nodes[4], self.nodes[7]]
        } else if key == self.nodes[7] {
            vec![self.nodes[8]]
        } else if key == self.nodes[8] {
            vec![self.nodes[9]]
        } else if key == self.nodes[9] {
            vec![self.nodes[10], self.nodes[11]]
        } else if key == self.nodes[11] {
            vec![self.nodes[12], self.nodes[13]]
        } else if key == self.nodes[14] {
            vec![self.nodes[15]]
        } else {
            return None;
        };
        children.get(usize::from(index)).copied()
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        if key == self.nodes[0] {
            Some(3)
        } else if key == self.nodes[7] {
            Some(1)
        } else {
            None
        }
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[1]).then_some(self.nodes[2])
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(0)
        } else if key == self.nodes[5] {
            Some(self.start)
        } else if key == self.nodes[6] {
            Some(self.end)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        self.nodes.contains(&key).then_some(types::I32)
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        if [
            self.nodes[1],
            self.nodes[10],
            self.nodes[12],
            self.nodes[15],
        ]
        .contains(&key)
        {
            Some(LocalSlotId {
                owner_node: 0,
                index: 0,
            })
        } else if key == self.nodes[3] || key == self.nodes[13] {
            Some(LocalSlotId {
                owner_node: 0,
                index: 1,
            })
        } else {
            None
        }
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.nodes[9]).then_some(LocalSlotId {
            owner_node: 0,
            index: 0,
        })
    }

    fn range_fact(&self, key: AstNodeKey) -> Option<RangeFact> {
        (key == self.nodes[4]).then_some(RangeFact::new(
            self.nodes[5],
            self.nodes[6],
            self.step,
            self.inclusive,
        ))
    }
}

fn range_facts(start: i64, end: i64, step: i64, inclusive: bool) -> RangeForFacts {
    RangeForFacts {
        nodes: keys("/tmp/RangeFor.bd", 21),
        start,
        end,
        step,
        inclusive,
    }
}

fn run_range(start: i64, end: i64, step: i64, inclusive: bool, index: u32) -> (i32, String) {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = range_facts(start, end, step, inclusive);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_statement(
            UserFuncName::user(0, index),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("verified range for");
    let clif = function.display().to_string();
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module
        .declare_function("range_for", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    (run(), clif)
}

#[test]
fn exclusive_range_for_executes_and_emits_stock_loop_clif() {
    let (result, clif) = run_range(1, 4, 1, false, 34);
    assert_eq!(result, 6);
    assert!(clif.contains("brif"), "{clif}");
    assert!(clif.contains("iadd_imm"), "{clif}");
}

#[test]
fn descending_inclusive_range_for_executes() {
    let (result, _) = run_range(3, 1, -1, true, 35);
    assert_eq!(result, 6);
}

#[test]
fn zero_step_range_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = range_facts(1, 4, 0, false);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_statement(
            UserFuncName::user(0, 36),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
        .expect_err("zero-step semantic range must not lower");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[3]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidRangeFor);
}
