//! `CallKind::Bulk` lowering: a `bulk`-parameter call packs N scalar arguments into a fresh
//! rooted array (reusing the `emit_array_literal` allocation/store/finish sequence) and
//! direct-calls the callee with that array as its sole argument.
//!
//! This mirrors `array_memory.rs` for the rooted-array allocation sequence and `direct_calls.rs`
//! for the call importer, combining both because `emit_bulk_call` emits runtime-helper imports
//! (TestCase-symbolic) AND a direct callee import (User-symbolic via the `CallImporter`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use beskid_isle::callee::DirectCallee;
use beskid_isle::syntax_types::{CallKind, LiteralKind};
use beskid_isle::{
    ArrayLayout, AstNodeKey, CallImportError, CallImporter, FunctionEmissionError, FunctionEmitter, LoweringErrorKind,
    ManagedArrayAllocation, NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{
    AbiParam, ExternalName, FuncRef, Function, GlobalValueData, Signature, UserFuncName, types,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

// The element count the fake allocator backs. The bulk call stores exactly three i32 scalars, so
// the backing buffer is sized to match; the callee sums them to prove every store landed.
const ELEMENT_COUNT: usize = 3;
static BULK_REQUEST: [u8; 32] = [0; 32];

unsafe extern "C" fn test_array_allocate_rooted(
    _request: *const u8,
    root_handle_out: *mut usize,
) -> *mut beskid_abi::BeskidArray {
    if !root_handle_out.is_null() {
        // SAFETY: fixture allocates the same pointer-sized output slot as ISLE lowering.
        unsafe { root_handle_out.write(0) };
    }
    let backing = Box::leak(vec![0_u8; ELEMENT_COUNT * std::mem::size_of::<i32>()].into_boxed_slice());
    Box::into_raw(Box::new(beskid_abi::BeskidArray {
        ptr: backing.as_mut_ptr(),
        len: ELEMENT_COUNT,
        cap: ELEMENT_COUNT,
    }))
}

unsafe extern "C" fn test_array_construction_finish(_root_handle: *mut u8) -> u8 {
    1
}

/// The bulk callee: receives the packed array pointer and sums its i32 elements. Proves the
/// call site packed every scalar and passed the array by value as the sole argument.
unsafe extern "C" fn sum_bulk(array: *mut beskid_abi::BeskidArray) -> i32 {
    let array = unsafe { &*array };
    let ptr = array.ptr as *const i32;
    let mut sum = 0_i32;
    for index in 0..array.len {
        sum = sum.wrapping_add(unsafe { *ptr.add(index) });
    }
    sum
}

/// Rebind test-only `ExternalName::TestCase` references (runtime helpers + the request-data
/// global) through concrete JIT-module declarations. The direct callee is imported via the
/// [`CallImporter`] as an `ExternalName::User` symbol, so it is left untouched here.
///
/// Mirrors `array_memory.rs::bind_array_fixture_symbols`: the emitter uses `TestCase` names while
/// building standalone CLIF, but `JITModule` only materializes relocations for module-owned
/// `User` imports.
fn bind_bulk_fixture_symbols(module: &mut JITModule, function: &mut Function) {
    let request = module
        .declare_data("__bulk_call_request", Linkage::Import, false, false)
        .expect("declare fixture request data");
    let request_global = module.declare_data_in_func(request, function);
    let request_global = function.global_values[request_global].clone();

    for (_, global) in function.global_values.iter_mut() {
        let GlobalValueData::Symbol { name: ExternalName::TestCase(name), .. } = global else {
            continue;
        };
        if name.raw() == b"__bulk_call_request" {
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

struct BulkCallFacts {
    call: AstNodeKey,
    arguments: [AstNodeKey; ELEMENT_COUNT],
    layout: ArrayLayout,
    callee: DirectCallee,
    signature: Signature,
}

impl NodeFacts for BulkCallFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.call {
            Some(NodeKind::CallExpression)
        } else if self.arguments.contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.arguments.contains(&key).then_some(LiteralKind::Integer)
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        (key == self.call).then_some(CallKind::Bulk)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        self.arguments.iter().position(|candidate| *candidate == key).map(|index| (index as i64 + 1) * 10)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.call || self.arguments.contains(&key)).then_some(types::I32)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.call).then_some(self.arguments.to_vec())
    }

    fn array_layout(&self, key: AstNodeKey) -> Option<ArrayLayout> {
        (key == self.call).then_some(self.layout)
    }

    fn managed_array_allocation(&self, key: AstNodeKey) -> Option<ManagedArrayAllocation> {
        (key == self.call).then_some(ManagedArrayAllocation { allocation_request_symbol: "__bulk_call_request".into() })
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        (key == self.call).then_some(self.callee.clone())
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.call).then_some(self.signature.clone())
    }
}

struct KnownCallImporter {
    module: JITModule,
    expected: DirectCallee,
}

impl CallImporter for KnownCallImporter {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, CallImportError> {
        if callee != self.expected {
            return Err(CallImportError::UnknownCallee);
        }
        let function = self
            .module
            .declare_function("sum_bulk", Linkage::Import, signature)
            .map_err(|_| CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
}

fn bulk_callee_key() -> AstNodeKey {
    let db = BeskidDatabase::default();
    AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/BulkCallee.bd")),
        generation: SyntaxGenerationId(17),
        node: AstNodeId(11),
    }
}

fn bulk_facts(isa: &dyn TargetIsa) -> BulkCallFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/BulkCall.bd"));
    let generation = SyntaxGenerationId(17);
    let pointer = isa.pointer_type();
    BulkCallFacts {
        call: AstNodeKey { unit, generation, node: AstNodeId(1) },
        arguments: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 2) }),
        layout: ArrayLayout::new(types::I32, 4, ELEMENT_COUNT as u32, 2),
        callee: DirectCallee::item(bulk_callee_key()),
        signature: Signature {
            params: vec![AbiParam::new(pointer)],
            returns: vec![AbiParam::new(types::I32)],
            call_conv: isa.default_call_conv(),
        },
    }
}

fn importer(isa: std::sync::Arc<dyn TargetIsa>, expected: DirectCallee) -> KnownCallImporter {
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("__bulk_call_request", BULK_REQUEST.as_ptr());
    builder.symbol("beskid_rt_v5_array_allocate_rooted", test_array_allocate_rooted as *const u8);
    builder.symbol("beskid_rt_v5_array_construction_finish", test_array_construction_finish as *const u8);
    builder.symbol("sum_bulk", sum_bulk as *const u8);
    KnownCallImporter { module: JITModule::new(builder), expected }
}

/// Define and run the emitted caller through the importer's own module. The direct callee was
/// imported during emission as a `User` symbol on this module, and the TestCase runtime helpers
/// + request global are rebound onto the same module just before definition.
fn execute_bulk_call(importer: &mut KnownCallImporter, name: &str, mut function: Function) -> i32 {
    bind_bulk_fixture_symbols(&mut importer.module, &mut function);
    let function_id =
        importer.module.declare_function(name, Linkage::Local, &function.signature).expect("declare caller");
    let mut context = importer.module.make_context();
    context.func = function;
    importer.module.define_function(function_id, &mut context).expect("define caller");
    importer.module.finalize_definitions().expect("finalize");
    let code = importer.module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    run()
}

#[test]
fn bulk_call_packs_scalars_into_a_rooted_array_and_direct_calls_the_callee() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = bulk_facts(isa.as_ref());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let function = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 31),
            signature.clone(),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect("verified bulk call");

    let clif = function.display().to_string();
    // The exact `emit_array_literal` allocation sequence is reused: rooted allocate, per-element
    // store, and construction finish must all appear. The bulk callee is then direct-called with
    // the packed array as its sole argument.
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(clif.contains("beskid_rt_v5_array_construction_finish"), "{clif}");
    assert!(clif.contains("store"), "{clif}");
    assert!(clif.contains("call"), "{clif}");
    // I32 elements are not pointer-typed, so no write barrier is emitted.
    assert!(!clif.contains("beskid_rt_v5_array_write_barrier"), "{clif}");

    assert_eq!(execute_bulk_call(&mut importer, "bulk_call", function), 60);
}

#[test]
fn bulk_call_rejects_an_arity_mismatch_between_layout_and_arguments() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let mut facts = bulk_facts(isa.as_ref());
    // Declare a layout for two elements while the call site passes three scalars.
    facts.layout = ArrayLayout::new(types::I32, 4, 2, 2);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let error = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 32),
            emitter.signature([], [types::I32]),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect_err("a layout/argument arity mismatch must fail before generating CLIF");

    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.call);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidArrayLayout);
}
