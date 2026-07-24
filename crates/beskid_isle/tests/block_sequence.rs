use std::path::PathBuf;

use beskid_isle::{AstNodeKey, FunctionEmitter, LiteralKind, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

struct BlockFacts {
    nodes: [AstNodeKey; 7],
}

impl NodeFacts for BlockFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::BlockExpression)
        } else if key == self.nodes[1] || key == self.nodes[3] {
            Some(NodeKind::ExpressionStatement)
        } else if key == self.nodes[5] {
            Some(NodeKind::ReturnStatement)
        } else if key == self.nodes[2] || key == self.nodes[4] || key == self.nodes[6] {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[2..].iter().step_by(2).any(|candidate| *candidate == key).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            [self.nodes[1], self.nodes[3], self.nodes[5]].get(usize::from(index)).copied()
        } else if key == self.nodes[1] {
            (index == 0).then_some(self.nodes[2])
        } else if key == self.nodes[3] {
            (index == 0).then_some(self.nodes[4])
        } else if key == self.nodes[5] {
            (index == 0).then_some(self.nodes[6])
        } else {
            None
        }
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[0]).then_some(3)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        self.nodes[2..].iter().step_by(2).position(|candidate| *candidate == key).map(|index| index as i64 + 1)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        self.nodes[2..].iter().step_by(2).any(|candidate| *candidate == key).then_some(types::I32)
    }
}

#[test]
fn block_rule_sequences_statements_and_returns_last_value() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Block.bd"));
    let generation = SyntaxGenerationId(11);
    let facts = BlockFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_statement(UserFuncName::user(0, 14), signature.clone(), &facts, facts.nodes[0])
        .expect("verified block sequence");

    let clif = function.display().to_string();
    let first = clif.find("iconst.i32 1").expect("first statement");
    let second = clif.find("iconst.i32 2").expect("second statement");
    let third = clif.find("iconst.i32 3").expect("return statement");
    assert!(first < second && second < third, "{clif}");

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module.declare_function("block_sequence", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 3);
}
