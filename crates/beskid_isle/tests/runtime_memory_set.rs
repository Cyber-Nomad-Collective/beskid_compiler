use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, CallKind, FunctionEmissionError, FunctionEmitter, LiteralKind, LoweringErrorKind, NodeFacts, NodeKind,
    RuntimeIntrinsicKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, Signature, UserFuncName, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use target_lexicon::Triple;

struct MemorySetFacts {
    nodes: [AstNodeKey; 5],
    pointer_type: cranelift_codegen::ir::Type,
}

struct CanonicalConstantMemorySetFacts {
    nodes: [AstNodeKey; 5],
    pointer_type: cranelift_codegen::ir::Type,
    canonical_constant: Option<i64>,
    length_is_path: bool,
}

struct RawWordStoreFacts {
    nodes: [AstNodeKey; 4],
    pointer_type: cranelift_codegen::ir::Type,
}

struct NestedRawWordStoreFacts {
    nodes: [AstNodeKey; 6],
    pointer_type: cranelift_codegen::ir::Type,
    offset_is_path: bool,
}

impl NodeFacts for RawWordStoreFacts {
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
        if key == self.nodes[0] { [self.nodes[1]].get(usize::from(index)).copied() } else { None }
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        (key == self.nodes[1]).then_some(CallKind::RuntimeIntrinsic)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        (key == self.nodes[1]).then_some(RuntimeIntrinsicKind::RawWordStore)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.nodes[1]).then(|| self.nodes[2..].to_vec())
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.nodes[1]).then(|| intrinsic_signature(&[self.pointer_type, self.pointer_type]))
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[2..].contains(&key).then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(0)
        } else if key == self.nodes[3] {
            Some(1)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        self.nodes[2..].contains(&key).then_some(self.pointer_type)
    }
}

impl NodeFacts for NestedRawWordStoreFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::ExpressionStatement)
        } else if key == self.nodes[1] || key == self.nodes[2] {
            Some(NodeKind::CallExpression)
        } else if key == self.nodes[4] {
            Some(if self.offset_is_path { NodeKind::PathExpression } else { NodeKind::LiteralExpression })
        } else if self.nodes[3] == key || self.nodes[5] == key {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.nodes[0] && index == 0).then_some(self.nodes[1])
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        (key == self.nodes[1] || key == self.nodes[2]).then_some(CallKind::RuntimeIntrinsic)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        if key == self.nodes[1] {
            Some(RuntimeIntrinsicKind::RawWordStore)
        } else if key == self.nodes[2] {
            Some(RuntimeIntrinsicKind::PointerAdd)
        } else {
            None
        }
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        if key == self.nodes[1] {
            Some(vec![self.nodes[2], self.nodes[5]])
        } else if key == self.nodes[2] {
            Some(vec![self.nodes[3], self.nodes[4]])
        } else {
            None
        }
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.nodes[1] || key == self.nodes[2])
            .then(|| intrinsic_signature(&[self.pointer_type, self.pointer_type]))
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.nodes[3] || key == self.nodes[5] || (key == self.nodes[4] && !self.offset_is_path))
            .then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[3] || key == self.nodes[5] {
            Some(0)
        } else if key == self.nodes[4] && !self.offset_is_path {
            Some(32)
        } else {
            None
        }
    }

    fn canonical_runtime_constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[4] && self.offset_is_path).then_some(32)
    }

    fn constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[4] && self.offset_is_path).then_some(32)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[4] {
            Some(types::I32)
        } else if self.nodes[1..].contains(&key) {
            Some(self.pointer_type)
        } else {
            None
        }
    }
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
        if key == self.nodes[0] { [self.nodes[1]].get(usize::from(index)).copied() } else { None }
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

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.nodes[1]).then(|| intrinsic_signature(&[self.pointer_type, types::I8, self.pointer_type]))
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[2..].contains(&key).then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(0)
        } else if key == self.nodes[3] {
            Some(0x1ff)
        } else if key == self.nodes[4] {
            Some(8)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[2] || key == self.nodes[4] {
            Some(self.pointer_type)
        } else if key == self.nodes[3] {
            Some(types::I64)
        } else {
            None
        }
    }
}

impl NodeFacts for CanonicalConstantMemorySetFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::ExpressionStatement)
        } else if key == self.nodes[1] {
            Some(NodeKind::CallExpression)
        } else if key == self.nodes[4] {
            Some(if self.length_is_path { NodeKind::PathExpression } else { NodeKind::LiteralExpression })
        } else if self.nodes[2..4].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] { [self.nodes[1]].get(usize::from(index)).copied() } else { None }
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

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.nodes[1]).then(|| intrinsic_signature(&[self.pointer_type, types::I8, self.pointer_type]))
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (self.nodes[2..4].contains(&key) || (!self.length_is_path && key == self.nodes[4]))
            .then_some(LiteralKind::Integer)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(0)
        } else if key == self.nodes[3] {
            Some(0x1ff)
        } else if !self.length_is_path && key == self.nodes[4] {
            Some(3480)
        } else {
            None
        }
    }

    fn constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[4]).then_some(self.canonical_constant).flatten()
    }

    fn canonical_runtime_constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[4]).then_some(self.canonical_constant).flatten()
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.nodes[2] {
            Some(self.pointer_type)
        } else if key == self.nodes[3] || key == self.nodes[4] {
            Some(types::I32)
        } else {
            None
        }
    }
}

fn facts(pointer_type: cranelift_codegen::ir::Type) -> MemorySetFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/RuntimeMemorySet.bd"));
    let generation = SyntaxGenerationId(401);
    MemorySetFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
    }
}

fn canonical_constant_facts(
    pointer_type: cranelift_codegen::ir::Type,
    canonical_constant: Option<i64>,
    length_is_path: bool,
) -> CanonicalConstantMemorySetFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/RuntimeMemorySetConstant.bd"));
    let generation = SyntaxGenerationId(404);
    CanonicalConstantMemorySetFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
        canonical_constant,
        length_is_path,
    }
}

fn raw_word_store_facts(pointer_type: cranelift_codegen::ir::Type) -> RawWordStoreFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/RuntimeRawWordStore.bd"));
    let generation = SyntaxGenerationId(402);
    RawWordStoreFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
    }
}

fn nested_raw_word_store_facts(
    pointer_type: cranelift_codegen::ir::Type,
    offset_is_path: bool,
) -> NestedRawWordStoreFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/RuntimeNestedRawWordStore.bd"));
    let generation = SyntaxGenerationId(406);
    NestedRawWordStoreFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
        offset_is_path,
    }
}

fn intrinsic_signature(params: &[cranelift_codegen::ir::Type]) -> Signature {
    let mut signature = Signature::new(CallConv::Fast);
    signature.params.extend(params.iter().copied().map(AbiParam::new));
    signature
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

#[test]
fn runtime_memory_set_materializes_a_canonical_constant_length_at_pointer_width() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = canonical_constant_facts(isa.pointer_type(), Some(3480), true);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 404), emitter.signature([], []), &facts, facts.nodes[0])
        .expect("canonical module constant length must materialize at the memory_set ABI word width");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 3480") || clif.contains("iconst.i32 3480"), "{clif}");
    assert!(clif.contains("store"), "memory_set must remain an inline store loop:\n{clif}");
    assert!(!clif.contains("beskid_rt_v5_intrinsic_memory_set"), "memory_set must not import an ABI helper:\n{clif}");
}

#[test]
fn runtime_memory_set_does_not_materialize_a_literal_length_as_a_runtime_constant() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    // Even a synthetic canonical-value claim cannot widen a literal. Only a
    // canonical PathExpression is eligible for the manifest ABI rule.
    let facts = canonical_constant_facts(isa.pointer_type(), Some(3480), false);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_statement(UserFuncName::user(0, 405), emitter.signature([], []), &facts, facts.nodes[0])
        .expect_err("literal lengths must not receive canonical runtime ABI materialization");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("literal must fail lowering rather than verification");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}

#[test]
fn runtime_raw_word_store_expression_statement_lowers_inline_without_an_abi_import() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = raw_word_store_facts(isa.pointer_type());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 403), emitter.signature([], []), &facts, facts.nodes[0])
        .expect("wrapped canonical runtime raw_word_store lowers as a statement");
    let clif = function.display().to_string();
    assert!(clif.contains("store"), "raw_word_store must lower to an inline store:\n{clif}");
    assert!(
        !clif.contains("beskid_rt_v5_intrinsic_raw_word_store"),
        "raw_word_store must not import an ABI helper:\n{clif}"
    );
}

#[test]
fn runtime_raw_word_store_materializes_a_nested_canonical_offset_at_pointer_width() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = nested_raw_word_store_facts(isa.pointer_type(), true);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 406), emitter.signature([], []), &facts, facts.nodes[0])
        .expect("nested canonical pointer_add offset must materialize at the intrinsic ABI word width");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 32") || clif.contains("iconst.i32 32"), "{clif}");
    assert!(clif.contains("iadd"), "pointer_add must remain inline:\n{clif}");
    assert!(clif.contains("store"), "raw_word_store must remain inline:\n{clif}");
}

#[test]
fn runtime_raw_word_store_does_not_materialize_a_nested_literal_offset() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = nested_raw_word_store_facts(isa.pointer_type(), false);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_statement(UserFuncName::user(0, 407), emitter.signature([], []), &facts, facts.nodes[0])
        .expect_err("literal offsets must not receive canonical runtime ABI materialization");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("literal offset must fail lowering rather than verification");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}
