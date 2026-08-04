use std::path::PathBuf;

use beskid_isle::{
    ArrayLayout, AstNodeKey, FunctionEmissionError, FunctionEmitter, LiteralKind, LoweringErrorKind,
    ManagedArrayAllocation, NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{ExternalName, Function, GlobalValueData, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use std::collections::BTreeMap;
use std::sync::Arc;
use target_lexicon::Triple;

// Static request symbol used by the source-only typed-array lowering acceptance fixture. The
// fake allocator below owns test backing storage; production requests are emitted by codegen.
static ARRAY_REQUEST: [u8; 32] = [0; 32];

unsafe extern "C" fn test_array_allocate_rooted(
    _request: *const u8,
    root_handle_out: *mut usize,
) -> *mut beskid_abi::BeskidArray {
    if !root_handle_out.is_null() {
        // SAFETY: fixture allocates the same pointer-sized output slot as ISLE lowering.
        unsafe { root_handle_out.write(0) };
    }
    let backing = Box::leak(vec![0_u8; 3 * std::mem::size_of::<i32>()].into_boxed_slice());
    Box::into_raw(Box::new(beskid_abi::BeskidArray { ptr: backing.as_mut_ptr(), len: 3, cap: 3 }))
}

unsafe extern "C" fn test_array_construction_finish(_root_handle: *mut u8) -> u8 {
    1
}

/// Rebind test-only symbolic references through the concrete JIT module.
///
/// The emitter deliberately uses `ExternalName::TestCase` while constructing standalone CLIF,
/// but `JITModule` only materializes relocations for module-owned `ExternalName::User` imports.
/// Rewriting the fixture's two runtime imports and its request-data global through module
/// declarations preserves an execution test on every host instead of treating the CLIF display
/// check as execution evidence.
fn bind_array_fixture_symbols(module: &mut JITModule, function: &mut Function) {
    let request = module
        .declare_data("__array_memory_request", Linkage::Import, false, false)
        .expect("declare fixture request data");
    let request_global = module.declare_data_in_func(request, function);
    let request_global = function.global_values[request_global].clone();

    for (_, global) in function.global_values.iter_mut() {
        let GlobalValueData::Symbol { name: ExternalName::TestCase(name), .. } = global else {
            continue;
        };
        if name.raw() == b"__array_memory_request" {
            *global = request_global.clone();
        }
    }

    let imports = function
        .dfg
        .ext_funcs
        .iter()
        .filter_map(|(_, import)| match &import.name {
            ExternalName::TestCase(name)
                if matches!(
                    name.raw(),
                    b"beskid_rt_v5_array_allocate_rooted" | b"beskid_rt_v5_array_construction_finish"
                ) =>
            {
                Some((
                    String::from_utf8(name.raw().to_vec()).expect("ASCII fixture import"),
                    function.dfg.signatures[import.signature].clone(),
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut bound_names = BTreeMap::<String, ExternalName>::new();
    for (symbol, signature) in imports {
        let id = module.declare_function(&symbol, Linkage::Import, &signature).expect("declare fixture import");
        let imported = module.declare_func_in_func(id, function);
        bound_names.insert(symbol, function.dfg.ext_funcs[imported].name.clone());
    }
    for (_, import) in function.dfg.ext_funcs.iter_mut() {
        let ExternalName::TestCase(name) = &import.name else {
            continue;
        };
        if let Some(bound) = bound_names.get(std::str::from_utf8(name.raw()).expect("ASCII fixture import")) {
            import.name = bound.clone();
        }
    }
}

fn execute_array_fixture(isa: Arc<dyn TargetIsa>, name: &str, mut function: Function) -> i32 {
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("__array_memory_request", ARRAY_REQUEST.as_ptr());
    builder.symbol("beskid_rt_v5_array_allocate_rooted", test_array_allocate_rooted as *const u8);
    builder.symbol("beskid_rt_v5_array_construction_finish", test_array_construction_finish as *const u8);
    let mut module = JITModule::new(builder);
    bind_array_fixture_symbols(&mut module, &mut function);
    let function_id = module.declare_function(name, Linkage::Local, &function.signature).expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(function_id, &mut context).expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    run()
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
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(!clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("trapnz"), "{clif}");
    assert!(clif.contains("load.i32"), "{clif}");

    assert_eq!(execute_array_fixture(isa, "array_index", function), 20);
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

    assert_eq!(execute_array_fixture(isa, "array_index_assign", function), 99);
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
