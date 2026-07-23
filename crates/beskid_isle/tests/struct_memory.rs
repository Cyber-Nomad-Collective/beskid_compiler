use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, FieldLayout, FunctionEmissionError, FunctionEmitter, LiteralKind,
    LoweringErrorKind, ManagedStructAllocation, NodeFacts, NodeKind, StructLayout,
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
}

impl NodeFacts for StructFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(match self.root {
                Root::Read => NodeKind::FieldExpression,
                Root::Write => NodeKind::AssignExpression,
            })
        } else if key == self.nodes[1] {
            Some(NodeKind::FieldExpression)
        } else if key == self.nodes[2] {
            Some(NodeKind::StructLiteralExpression)
        } else if self.nodes[3..].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[3..]
            .contains(&key)
            .then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            match self.root {
                Root::Read => [self.nodes[2]].get(usize::from(index)).copied(),
                Root::Write => [self.nodes[1], self.nodes[6]]
                    .get(usize::from(index))
                    .copied(),
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
        (key == self.nodes[0] || key == self.nodes[1] || key == self.nodes[2])
            .then(|| self.layout.clone())
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        (key == self.nodes[2]).then(|| ManagedStructAllocation {
            allocation_request_symbol: "__test_struct_allocation_request".into(),
        })
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
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation,
            node: AstNodeId(index as u32 + 1),
        }),
        pointer_type,
        layout,
        root,
        field_index,
    }
}

fn valid_layout() -> StructLayout {
    StructLayout::new(
        32,
        2,
        vec![
            FieldLayout::new(types::I32, 16),
            FieldLayout::new(types::I32, 20),
            FieldLayout::new(types::I32, 24),
        ],
    )
}

fn emit(root: Root, field_index: u32, function_index: u32) -> String {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), root, field_index, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(
            UserFuncName::user(0, function_index),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("verified struct field lowering");
    function.display().to_string()
}

#[test]
fn struct_literal_and_field_read_emit_managed_clif() {
    let clif = emit(Root::Read, 1, 22);
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    assert!(!clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn field_assignment_emits_managed_clif_store() {
    let clif = emit(Root::Write, 1, 23);
    assert!(
        clif.lines()
            .any(|line| line.trim_start().starts_with("store ")),
        "{clif}"
    );
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
        .emit_expression(
            UserFuncName::user(0, 24),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
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
        .emit_expression(
            UserFuncName::user(0, 25),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
        .expect_err("field index must exist in semantic layout");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidStructField(3));
}
