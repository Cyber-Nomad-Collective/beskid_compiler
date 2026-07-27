use std::path::PathBuf;

use beskid_isle::{AstNodeKey, CallKind, FunctionEmitter, LiteralKind, NodeFacts, NodeKind, RuntimeIntrinsicKind};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use target_lexicon::Triple;

struct MemorySetFacts {
    nodes: [AstNodeKey; 5],
    pointer_type: cranelift_codegen::ir::Type,
}

impl NodeFacts for MemorySetFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::ExpressionStatement)
        } else if key == self.nodes[1] {
            Some(NodeKind::CallExpression)
        } else if self.nodes[2..].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            [self.nodes[1]].get(usize::from(index)).copied()
        } else {
            None
        }
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        (key == self.nodes[1]).then_some(CallKind::RuntimeIntrinsic)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        (key == self.nodes[1]).then_some(RuntimeIntrinsicKind::MemorySet)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.nodes[1]).then(|| self.nodes[2..].to_vec())
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[2..].contains(&key).then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] { Some(0) } else if key == self.nodes[3] { Some(0x1ff) } else if key == self.nodes[4] { Some(8) } else { None }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[2] || key == self.nodes[4] { Some(self.pointer_type) } else if key == self.nodes[3] { Some(types::I64) } else { None }
    }
}

fn facts(pointer_type: cranelift_codegen::ir::Type) -> MemorySetFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/RuntimeMemorySet.bd"));
    let generation = SyntaxGenerationId(401);
    MemorySetFacts { nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }), pointer_type }
}

#[test]
fn runtime_memory_set_reduces_word_byte_to_verified_i8_store() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 401), emitter.signature([], []), &facts, facts.nodes[0])
        .expect("manifest-authorized runtime memory_set with a word byte lowers");
    let clif = function.display().to_string();
    assert!(clif.contains("ireduce.i8"), "the ABI word byte must be reduced before the i8 store:\n{clif}");
    assert!(clif.contains("store"), "memory_set must still lower to an inline store loop:\n{clif}");
    assert!(!clif.contains("beskid_rt_v5_intrinsic_memory_set"), "memory_set must stay inline:\n{clif}");
}

#[test]
fn runtime_memory_set_call_statement_reduces_word_byte_without_an_abi_import() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 402), emitter.signature([], []), &facts, facts.nodes[1])
        .expect("naked canonical runtime memory_set call lowers as a statement");
    let clif = function.display().to_string();
    assert!(clif.contains("ireduce.i8"), "the ABI word byte must be reduced before the i8 store:\n{clif}");
    assert!(clif.contains("store"), "memory_set must lower to an inline store loop:\n{clif}");
    assert!(!clif.contains("beskid_rt_v5_intrinsic_memory_set"), "memory_set must not import an ABI helper:\n{clif}");
}
