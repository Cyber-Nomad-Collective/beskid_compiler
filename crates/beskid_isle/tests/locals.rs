use std::path::PathBuf;

use beskid_isle::{AstNodeKey, FunctionEmitter, LiteralKind, LocalSlotId, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

struct LocalFacts {
    nodes: [AstNodeKey; 9],
}

impl NodeFacts for LocalFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let kind = if key == self.nodes[0] {
            NodeKind::BlockExpression
        } else if key == self.nodes[1] {
            NodeKind::LetStatement
        } else if key == self.nodes[3] {
            NodeKind::ExpressionStatement
        } else if key == self.nodes[4] {
            NodeKind::AssignExpression
        } else if key == self.nodes[5] || key == self.nodes[8] {
            NodeKind::PathExpression
        } else if key == self.nodes[7] {
            NodeKind::ReturnStatement
        } else if key == self.nodes[2] || key == self.nodes[6] {
            NodeKind::LiteralExpression
        } else {
            return None;
        };
        Some(kind)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.nodes[2] || key == self.nodes[6]).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            [self.nodes[1], self.nodes[3], self.nodes[7]].get(usize::from(index)).copied()
        } else if key == self.nodes[3] && index == 0 {
            Some(self.nodes[4])
        } else if key == self.nodes[4] {
            [self.nodes[5], self.nodes[6]].get(usize::from(index)).copied()
        } else if key == self.nodes[7] && index == 0 {
            Some(self.nodes[8])
        } else {
            None
        }
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[0]).then_some(3)
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[1]).then_some(self.nodes[2])
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(1)
        } else if key == self.nodes[6] {
            Some(2)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.nodes[1] || key == self.nodes[2] || key == self.nodes[6]).then_some(types::I32)
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.nodes[1] || key == self.nodes[5] || key == self.nodes[8])
            .then_some(LocalSlotId { owner_node: 0, index: 0 })
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.nodes[4]).then_some(LocalSlotId { owner_node: 0, index: 0 })
    }
}

#[test]
fn let_assign_and_path_rules_share_one_ssa_local() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Locals.bd"));
    let generation = SyntaxGenerationId(14);
    let facts = LocalFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_statement(UserFuncName::user(0, 17), signature.clone(), &facts, facts.nodes[0])
        .expect("verified local assignment");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 1"), "{clif}");
    assert!(clif.contains("iconst.i32 2"), "{clif}");

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module.declare_function("locals", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 2);
}
