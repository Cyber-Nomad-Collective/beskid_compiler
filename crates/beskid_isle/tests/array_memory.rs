use std::path::PathBuf;

use beskid_isle::{
    ArrayLayout, AstNodeKey, FunctionEmissionError, FunctionEmitter, LiteralKind, LoweringErrorKind,
    ManagedArrayAllocation, NodeFacts,
    NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

// Static request symbol used by the source-only typed-array lowering acceptance fixture. The
// fake allocator below owns test backing storage; production requests are emitted by codegen.
static ARRAY_REQUEST: [u8; 32] = [0; 32];

unsafe extern "C" fn test_array_allocate(_request: *const u8) -> *mut beskid_abi::BeskidArray {
    let backing = Box::leak(vec![0_u8; 3 * std::mem::size_of::<i32>()].into_boxed_slice());
    Box::into_raw(Box::new(beskid_abi::BeskidArray { ptr: backing.as_mut_ptr(), len: 3, cap: 3 }))
}

struct ArrayFacts {
    nodes: [AstNodeKey; 8],
    pointer_type: cranelift_codegen::ir::Type,
    layout: ArrayLayout,
    root: Root,
}

#[derive(Clone, Copy)]
enum Root {
    IndexRead,
    IndexAssign,
}

impl NodeFacts for ArrayFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            match self.root {
                Root::IndexRead => Some(NodeKind::IndexExpression),
                Root::IndexAssign => Some(NodeKind::AssignExpression),
            }
        } else if key == self.nodes[1] {
            match self.root {
                Root::IndexRead => Some(NodeKind::ArrayLiteralExpression),
                Root::IndexAssign => Some(NodeKind::IndexExpression),
            }
        } else if key == self.nodes[2] {
            match self.root {
                Root::IndexRead => Some(NodeKind::LiteralExpression),
                Root::IndexAssign => Some(NodeKind::ArrayLiteralExpression),
            }
        } else if self.nodes[3..].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        match self.root {
            Root::IndexRead => self.nodes[2..].contains(&key).then_some(LiteralKind::Integer),
            Root::IndexAssign => self.nodes[3..].contains(&key).then_some(LiteralKind::Integer),
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        match self.root {
            Root::IndexRead if key == self.nodes[0] => [self.nodes[1], self.nodes[2]].get(usize::from(index)).copied(),
            Root::IndexAssign if key == self.nodes[0] => {
                [self.nodes[1], self.nodes[3]].get(usize::from(index)).copied()
            }
            Root::IndexAssign if key == self.nodes[1] => {
                [self.nodes[2], self.nodes[4]].get(usize::from(index)).copied()
            }
            _ => None,
        }
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        match self.root {
            Root::IndexRead if key == self.nodes[2] => Some(1),
            Root::IndexRead => {
                self.nodes[3..].iter().position(|candidate| *candidate == key).map(|index| (index as i64 + 1) * 10)
            }
            // Assign: nodes[3]=stored value 99, nodes[4]=index 1, nodes[5..]=literal elems 10,20,30
            Root::IndexAssign if key == self.nodes[3] => Some(99),
            Root::IndexAssign if key == self.nodes[4] => Some(1),
            Root::IndexAssign => {
                self.nodes[5..].iter().position(|candidate| *candidate == key).map(|index| (index as i64 + 1) * 10)
            }
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        match self.root {
            Root::IndexRead if key == self.nodes[1] => Some(self.pointer_type),
            Root::IndexRead if key == self.nodes[0] || self.nodes[2..].contains(&key) => Some(types::I32),
            Root::IndexAssign if key == self.nodes[2] => Some(self.pointer_type),
            Root::IndexAssign if key == self.nodes[0] || key == self.nodes[1] || self.nodes[3..].contains(&key) => {
                Some(types::I32)
            }
            _ => None,
        }
    }

    fn array_elements(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        match self.root {
            Root::IndexRead if key == self.nodes[1] => Some(self.nodes[3..6].to_vec()),
            Root::IndexAssign if key == self.nodes[2] => Some(self.nodes[5..8].to_vec()),
            _ => None,
        }
    }

    fn array_layout(&self, key: AstNodeKey) -> Option<ArrayLayout> {
        match self.root {
            Root::IndexRead if key == self.nodes[0] || key == self.nodes[1] => Some(self.layout),
            Root::IndexAssign if key == self.nodes[1] || key == self.nodes[2] => Some(self.layout),
            _ => None,
        }
    }

    fn managed_array_allocation(&self, key: AstNodeKey) -> Option<ManagedArrayAllocation> {
        match self.root {
            Root::IndexRead if key == self.nodes[1] => {
                Some(ManagedArrayAllocation { allocation_request_symbol: "__array_memory_request".into() })
            }
            Root::IndexAssign if key == self.nodes[2] => {
                Some(ManagedArrayAllocation { allocation_request_symbol: "__array_memory_request".into() })
            }
            _ => None,
        }
    }
}

fn facts(pointer_type: cranelift_codegen::ir::Type, length: u32, root: Root) -> ArrayFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Array.bd"));
    let generation = SyntaxGenerationId(16);
    ArrayFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
        layout: ArrayLayout::new(types::I32, 4, length, 2),
        root,
    }
}

#[test]
fn array_literal_and_index_emit_checked_stock_clif_and_execute() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), 3, Root::IndexRead);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(UserFuncName::user(0, 20), signature.clone(), &facts, facts.nodes[0])
        .expect("verified array index");
    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_array_allocate"), "{clif}");
    assert!(!clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("trapnz"), "{clif}");
    assert!(clif.contains("load.i32"), "{clif}");

    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("__array_memory_request", ARRAY_REQUEST.as_ptr());
    builder.symbol("beskid_rt_v5_array_allocate", test_array_allocate as *const u8);
    let mut module = JITModule::new(builder);
    let function_id = module.declare_function("array_index", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 20);
}

#[test]
fn array_index_assignment_emits_checked_stock_clif_store_and_executes() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), 3, Root::IndexAssign);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(UserFuncName::user(0, 22), signature.clone(), &facts, facts.nodes[0])
        .expect("verified array index assign");
    let clif = function.display().to_string();
    assert!(clif.contains("trapnz"), "{clif}");
    assert!(clif.contains("store"), "{clif}");
    assert!(!clif.contains("load.i32"), "{clif}");

    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("__array_memory_request", ARRAY_REQUEST.as_ptr());
    builder.symbol("beskid_rt_v5_array_allocate", test_array_allocate as *const u8);
    let mut module = JITModule::new(builder);
    let function_id = module.declare_function("array_index_assign", Linkage::Local, &signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 99);
}

#[test]
fn mismatched_array_layout_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), 2, Root::IndexRead);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 21), emitter.signature([], [isa.pointer_type()]), &facts, facts.nodes[1])
        .expect_err("layout length must match semantic element facts");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[1]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidArrayLayout);
}
