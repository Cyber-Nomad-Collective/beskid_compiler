use std::path::PathBuf;

use beskid_isle::syntax_types::LiteralKind;
use beskid_isle::{
    AssignmentKind, AstNodeKey, FieldLayout, FunctionEmissionError, FunctionEmitter, LoweringErrorKind,
    ManagedStructAllocation, NodeFacts, NodeKind, StructLayout,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{Type, UserFuncName, types};
use cranelift_codegen::settings;
use target_lexicon::Triple;

#[derive(Clone, Copy)]
enum Root {
    Read,
    Write,
}

struct StructFacts {
    nodes: [AstNodeKey; 7],
    pointer_type: Type,
    layout: StructLayout,
    root: Root,
    field_index: u32,
    unit_middle: bool,
    unit_effect: Option<cranelift_codegen::ir::Signature>,
}

impl NodeFacts for StructFacts {
    fn assignment_kind(&self, key: AstNodeKey) -> Option<AssignmentKind> {
        (matches!(self.root, Root::Write) && key == self.nodes[0]).then_some(AssignmentKind::Field)
    }

    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key.node == AstNodeId(99) {
            Some(NodeKind::ExpressionStatement)
        } else if key == self.nodes[0] {
            Some(match self.root {
                Root::Read => NodeKind::FieldExpression,
                Root::Write => NodeKind::AssignExpression,
            })
        } else if key == self.nodes[1] {
            Some(NodeKind::FieldExpression)
        } else if key == self.nodes[2] {
            Some(NodeKind::StructLiteralExpression)
        } else if key == self.nodes[4] && self.unit_effect.is_some() {
            Some(NodeKind::CallExpression)
        } else if self.nodes[3..].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[3..].contains(&key).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key.node == AstNodeId(99) {
            return (index == 0).then_some(self.nodes[0]);
        }
        if key == self.nodes[0] {
            match self.root {
                Root::Read => [self.nodes[2]].get(usize::from(index)).copied(),
                Root::Write => [self.nodes[1], self.nodes[6]].get(usize::from(index)).copied(),
            }
        } else if key == self.nodes[1] {
            [self.nodes[2]].get(usize::from(index)).copied()
        } else {
            None
        }
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[3] {
            Some(10)
        } else if key == self.nodes[4] {
            Some(20)
        } else if key == self.nodes[5] {
            Some(30)
        } else if key == self.nodes[6] {
            Some(99)
        } else {
            None
        }
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<beskid_isle::syntax_types::CallKind> {
        (key == self.nodes[4] && self.unit_effect.is_some()).then_some(beskid_isle::syntax_types::CallKind::Direct)
    }
    fn direct_callee(&self, key: AstNodeKey) -> Option<beskid_isle::callee::DirectCallee> {
        self.call_kind(key).map(|_| beskid_isle::callee::DirectCallee::item(self.nodes[4]))
    }
    fn call_signature(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Signature> {
        self.call_kind(key).and(self.unit_effect.clone())
    }
    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.call_kind(key).map(|_| Vec::new())
    }

    fn semantic_type(&self, key: AstNodeKey) -> Option<beskid_queries::SemanticTypeId> {
        (self.unit_middle && (key == self.nodes[4]
            || (self.field_index == 1 && (key == self.nodes[0] || key == self.nodes[1]
                || (matches!(self.root, Root::Write) && key == self.nodes[6])))))
            .then_some(beskid_queries::SemanticTypeId::UNIT)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        if key == self.nodes[2] {
            Some(self.pointer_type)
        } else if self.nodes.contains(&key) {
            Some(types::I32)
        } else {
            None
        }
    }

    fn struct_fields(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.nodes[2]).then(|| self.nodes[3..6].to_vec())
    }

    fn struct_layout(&self, key: AstNodeKey) -> Option<StructLayout> {
        (key == self.nodes[0] || key == self.nodes[1] || key == self.nodes[2]).then(|| self.layout.clone())
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        (key == self.nodes[2])
            .then(|| ManagedStructAllocation { allocation_request_symbol: "__test_struct_allocation_request".into() })
    }

    fn field_index(&self, key: AstNodeKey) -> Option<u32> {
        (key == self.nodes[0] || key == self.nodes[1]).then_some(self.field_index)
    }
}

fn facts(pointer_type: Type, root: Root, field_index: u32, layout: StructLayout) -> StructFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Struct.bd"));
    let generation = SyntaxGenerationId(17);
    StructFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
        layout,
        root,
        field_index,
        unit_middle: false,
        unit_effect: None,
    }
}

fn valid_layout() -> StructLayout {
    StructLayout::new(
        32,
        2,
        vec![FieldLayout::new(types::I32, 16), FieldLayout::new(types::I32, 20), FieldLayout::new(types::I32, 24)],
    )
}

fn emit(root: Root, field_index: u32, function_index: u32) -> cranelift_codegen::ir::Function {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), root, field_index, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    emitter
        .emit_expression(UserFuncName::user(0, function_index), signature.clone(), &facts, facts.nodes[0])
        .expect("verified struct field lowering")
}

#[test]
fn struct_literal_and_field_read_emit_managed_clif() {
    let function = emit(Root::Read, 1, 22);
    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    assert!(function.sized_stack_slots.is_empty(), "managed object fields must not use stack storage: {clif}");
    let memory = function
        .layout
        .blocks()
        .flat_map(|block| function.layout.block_insts(block))
        .filter_map(|inst| function.dfg.insts[inst].memflags_data(&function.dfg))
        .collect::<Vec<_>>();
    assert!(!memory.is_empty(), "fixture must access managed fields");
    assert!(
        memory.iter().all(|flags| !flags.notrap() && !flags.aligned()),
        "managed field accesses retain untrusted flags: {clif}"
    );
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn field_assignment_emits_managed_clif_store() {
    let clif = emit(Root::Write, 1, 23).display().to_string();
    assert!(clif.lines().any(|line| line.trim_start().starts_with("store ")), "{clif}");
}

#[test]
fn invalid_struct_layout_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let invalid = StructLayout::new(8, 2, vec![FieldLayout::new(types::I32, 8)]);
    let facts = facts(isa.pointer_type(), Root::Read, 0, invalid);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 24), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .expect_err("field extends beyond semantic struct size");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidStructLayout);
}

#[test]
fn missing_struct_field_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Root::Read, 3, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 25), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .expect_err("field index must exist in semantic layout");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidStructField(3));
}

#[test]
fn unit_field_keeps_logical_indices_without_a_physical_store() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .unwrap()
        .finish(settings::Flags::new(settings::builder()))
        .unwrap();
    let layout = StructLayout::from_logical_fields(
        24,
        3,
        vec![Some(FieldLayout::new(types::I32, 16)), None, Some(FieldLayout::new(types::I32, 20))],
    );
    let mut facts = facts(isa.pointer_type(), Root::Read, 2, layout);
    facts.unit_middle = true;
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(UserFuncName::user(0, 26), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .unwrap();
    let clif = function.display().to_string();
    assert_eq!(clif.lines().filter(|line| line.trim_start().starts_with("store ")).count(), 2, "{clif}");
    assert!(clif.contains("load.i32") && clif.contains("+20"), "{clif}");
}

#[test]
fn no_storage_field_rejects_nonunit_initializer() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .unwrap()
        .finish(settings::Flags::new(settings::builder()))
        .unwrap();
    let layout = StructLayout::from_logical_fields(
        24,
        3,
        vec![Some(FieldLayout::new(types::I32, 16)), None, Some(FieldLayout::new(types::I32, 20))],
    );
    let facts = facts(isa.pointer_type(), Root::Read, 2, layout);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 27), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .unwrap_err();
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering denial");
    };
    assert_eq!(error.key(), facts.nodes[2]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidStructLayout);
}

struct UnitEffectImporter {
    module: cranelift_jit::JITModule,
    imports: usize,
}
impl beskid_isle::CallImporter for UnitEffectImporter {
    fn import(
        &mut self,
        builder: &mut cranelift_frontend::FunctionBuilder<'_>,
        _callee: beskid_isle::callee::DirectCallee,
        signature: &cranelift_codegen::ir::Signature,
    ) -> Result<cranelift_codegen::ir::FuncRef, beskid_isle::CallImportError> {
        use cranelift_module::Module;
        self.imports += 1;
        let id = self
            .module
            .declare_function("unit_initializer_effect", cranelift_module::Linkage::Import, signature)
            .map_err(|_| beskid_isle::CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(id, builder.func))
    }
}

#[test]
fn unit_initializer_call_is_emitted_once_before_allocation_without_storage() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .unwrap()
        .finish(settings::Flags::new(settings::builder()))
        .unwrap();
    let layout = StructLayout::from_logical_fields(
        24,
        3,
        vec![Some(FieldLayout::new(types::I32, 16)), None, Some(FieldLayout::new(types::I32, 20))],
    );
    let mut facts = facts(isa.pointer_type(), Root::Read, 2, layout);
    facts.unit_middle = true;
    facts.unit_effect = Some(cranelift_codegen::ir::Signature::new(isa.default_call_conv()));
    let module = cranelift_jit::JITModule::new(cranelift_jit::JITBuilder::with_isa(
        isa.clone(),
        cranelift_module::default_libcall_names(),
    ));
    let mut importer = UnitEffectImporter { module, imports: 0 };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 28),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
            &mut importer,
        )
        .unwrap();
    let clif = function.display().to_string();
    assert_eq!(importer.imports, 1, "unit initializer must lower exactly once: {clif}");
    assert_eq!(clif.lines().filter(|line| line.trim_start().starts_with("store ")).count(), 2, "{clif}");
    let calls = clif.lines().filter(|line| line.contains("call ")).collect::<Vec<_>>();
    assert_eq!(calls.len(), 2, "one initializer call followed by one allocation: {clif}");
}

#[test]
fn unit_field_read_and_assignment_evaluate_receiver_without_payload_access() {
    let isa = cranelift_codegen::isa::lookup(Triple::host()).unwrap()
        .finish(settings::Flags::new(settings::builder())).unwrap();
    for (index, root) in [Root::Read, Root::Write].into_iter().enumerate() {
        let layout = StructLayout::from_logical_fields(24, 3, vec![
            Some(FieldLayout::new(types::I32, 16)), None, Some(FieldLayout::new(types::I32, 20)),
        ]);
        let mut facts = facts(isa.pointer_type(), root, 1, layout);
        facts.unit_middle = true;
        let emitter = FunctionEmitter::new(isa.as_ref());
        let function = emitter.emit_statement(UserFuncName::user(0, 29 + index as u32),
            emitter.signature([], []), &facts, AstNodeKey { node: AstNodeId(99), ..facts.nodes[0] }).unwrap();
        let clif = function.display().to_string();
        assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "receiver evaluated: {clif}");
        assert_eq!(clif.lines().filter(|line| line.trim_start().starts_with("store ")).count(), 2, "{clif}");
        assert!(!clif.contains("load.i32"), "unit has no payload load: {clif}");
    }
}
