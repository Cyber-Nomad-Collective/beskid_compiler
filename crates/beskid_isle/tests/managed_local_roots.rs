use std::path::PathBuf;

use beskid_isle::{AstNodeKey, FunctionEmitter, LocalSlotId, ManagedReferenceFact, NodeFacts, NodeKind, ParameterSlot};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::UserFuncName;
use cranelift_codegen::settings;
use target_lexicon::Triple;

struct ManagedParameterFacts {
    item: AstNodeKey,
    body: AstNodeKey,
    result: AstNodeKey,
}

impl NodeFacts for ManagedParameterFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.body {
            Some(NodeKind::ReturnStatement)
        } else if key == self.result {
            Some(NodeKind::PathExpression)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.body && index == 0).then_some(self.result)
    }

    fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.result).then_some(cranelift_codegen::ir::types::I64)
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.result).then_some(LocalSlotId { owner_node: self.item.node.0, index: 0 })
    }

    fn function_parameters(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        (key == self.item).then_some(vec![ParameterSlot {
            slot: LocalSlotId { owner_node: self.item.node.0, index: 0 },
            value_type: cranelift_codegen::ir::types::I64,
            managed_reference: ManagedReferenceFact::GcManaged,
        }])
    }
}

#[test]
fn managed_leaf_parameter_returns_without_registering_a_slot() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let database = BeskidDatabase::default();
    let unit = SourceUnitId::new(&database, PathBuf::from("/tmp/ManagedParameter.bd"));
    let generation = SyntaxGenerationId(1);
    let facts = ManagedParameterFacts {
        item: AstNodeKey { unit, generation, node: AstNodeId(1) },
        body: AstNodeKey { unit, generation, node: AstNodeId(2) },
        result: AstNodeKey { unit, generation, node: AstNodeId(3) },
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let pointer = isa.pointer_type();
    let function = emitter
        .emit_item_statement(
            UserFuncName::user(0, 61),
            emitter.signature([pointer], [pointer]),
            &facts,
            facts.item,
            facts.body,
        )
        .expect("managed parameter lowering");
    let clif = function.display().to_string();
    assert!(function.sized_stack_slots.is_empty(), "{clif}");
    assert!(!clif.contains("gc_register_root"), "{clif}");
    assert!(!clif.contains("gc_unregister_root"), "{clif}");
}
