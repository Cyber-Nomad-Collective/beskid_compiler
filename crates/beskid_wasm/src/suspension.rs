//! Browser ABI admission for the canonical Corelib suspension service.

use anyhow::{Result, ensure};
use beskid_abi::runtime_source::{CorelibServiceAbiType, canonical_corelib_service_abi_for_adapter};
use cranelift_codegen::ir::{AbiParam, Signature, types::I64};
use std::collections::HashSet;

// The canonical `__fiber_yield` facade lowers to the ABI-v5 runtime adapter.
pub(crate) const FIBER_YIELD: &str = "beskid_rt_v5_fiber_yield";
pub(crate) const SLEEP_UNTIL: &str = "beskid_rt_v5_external_sleep_until";
pub(crate) const HOST_MODULE: &str = "beskid";

pub(crate) fn is_host_import(name: &str, signature: &Signature, services: &HashSet<String>) -> Result<bool> {
    use CorelibServiceAbiType::{I64 as ManifestI64, Usize, Void};
    let (parameters, result) = match name {
        FIBER_YIELD => (&[][..], Void),
        SLEEP_UNTIL => (&[ManifestI64][..], Usize),
        _ => return Ok(false),
    };
    ensure!(services.contains(name), "browser service {name} requires its compiler-authorized Corelib declaration");
    ensure!(
        canonical_corelib_service_abi_for_adapter(name)
            .is_some_and(|abi| abi.parameters == parameters && abi.result == result),
        "canonical Corelib service {name} has a different suspension ABI"
    );
    let scalar = AbiParam::new(I64);
    let expected = if name == FIBER_YIELD { &[][..] } else { &[scalar][..] };
    ensure!(
        signature.params == expected && signature.returns == expected,
        "browser service {name} has an invalid signature"
    );
    Ok(true)
}

pub(crate) fn signature(name: &str) -> Option<(&'static [wasm_encoder::ValType], &'static [wasm_encoder::ValType])> {
    use wasm_encoder::ValType::I64;
    match name {
        FIBER_YIELD => Some((&[], &[])),
        SLEEP_UNTIL => Some((&[I64], &[I64])),
        crate::fibers::JOIN_STATUS => Some((&[I64], &[wasm_encoder::ValType::I32])),
        crate::foundation::FILE_READ | crate::foundation::FILE_WRITE => Some((&[I64, I64], &[wasm_encoder::ValType::I32])),
        name if crate::foundation::FILES.contains(&name) => Some((&[I64], &[wasm_encoder::ValType::I32])),
        crate::stdio::READ | crate::stdio::READ_BYTES => Some((&[wasm_encoder::ValType::I32, I64, I64], &[I64])),
        _ => crate::scheduler::find(name).filter(|service| service.suspends).map(|service| service.wasm())
            .or_else(|| crate::protocol::signature(name))
            .or_else(|| crate::network::suspension_signature(name)),
    }
}

/// After JSPI unwinds a call to the host, Wasm keeps its local reference alive.
/// Restore that arena before translated CLIF can next use an integer handle.
pub(crate) fn encode_scoped_imports(names: &[&str]) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection, Module, RefType,
        TypeSection, ValType,
    };

    let eqref = ValType::Ref(RefType::EQREF);
    let mut types = TypeSection::new();
    types.ty().function([], [eqref]);
    types.ty().function([eqref], []);
    types.ty().function([], [ValType::I64]);
    types.ty().function([ValType::I64], []);
    let mut imports = ImportSection::new();
    imports.import("heap", crate::arena::SCOPE_GET, EntityType::Function(0));
    imports.import("heap", crate::arena::SCOPE_SET, EntityType::Function(1));
    imports.import("heap", crate::arena::FIBER_GET, EntityType::Function(2));
    imports.import("heap", crate::arena::FIBER_SET, EntityType::Function(3));
    for name in names {
        let (params, results) = signature(name).expect("verified suspending service");
        let index = types.len();
        types.ty().function(params.iter().copied(), results.iter().copied());
        imports.import(HOST_MODULE, name, EntityType::Function(index));
    }

    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    for (ordinal, name) in names.iter().enumerate() {
        let index = ordinal as u32 + 4;
        let arity = signature(name).expect("verified suspending service").0.len() as u32;
        functions.function(index);
        exports.export(name, ExportKind::Func, imports.len() + ordinal as u32);
        let mut function = Function::new_with_locals_types([eqref, ValType::I64]);
        let mut body = function.instructions();
        body.call(0).local_set(arity).call(2).local_set(arity + 1);
        // The host copies managed arguments and detaches this scope before awaiting.
        for parameter in 0..arity {
            body.local_get(parameter);
        }
        body.call(index).local_get(arity).call(1).local_get(arity + 1).call(3).end();
        code.function(&function);
    }

    let mut module = Module::new();
    module.section(&types).section(&imports).section(&functions).section(&exports).section(&code);
    module.finish()
}
