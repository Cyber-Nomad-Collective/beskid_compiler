use std::collections::{HashMap, HashSet};

use beskid_codegen::CodegenArtifact;
use cranelift_codegen::ir::ExternalName;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use super::errors::JitError;

pub(super) fn declare_exact_runtime_imports(
    module: &mut JITModule,
    artifact: &CodegenArtifact,
    exact_symbols: &HashSet<String>,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), JitError> {
    for function in &artifact.functions {
        for (_, external) in function.function.dfg.ext_funcs.iter() {
            let ExternalName::TestCase(name) = &external.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw());
            if !exact_symbols.contains(symbol.as_ref()) || func_ids.contains_key(symbol.as_ref()) {
                continue;
            }
            let signature = function.function.dfg.signatures[external.signature].clone();
            beskid_codegen::cranelift_host::validate_ffi_signature(&signature, module.isa().pointer_type())
                .map_err(JitError::Isa)?;
            let id = module.declare_function(symbol.as_ref(), Linkage::Import, &signature)?;
            func_ids.insert(symbol.into_owned(), id);
        }
    }
    Ok(())
}

/// Declare TestCase externals that match the runtime kit import_allowlist but aren't
/// already declared (e.g. C library math functions from clif blocks).
pub(super) fn declare_import_allowlist_symbols(
    module: &mut JITModule,
    artifact: &CodegenArtifact,
    allowlist: &HashSet<String>,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), JitError> {
    for function in &artifact.functions {
        for (_, ext_func) in function.function.dfg.ext_funcs.iter() {
            let cranelift_codegen::ir::ExternalName::TestCase(name) = &ext_func.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw()).to_string();
            if func_ids.contains_key(&symbol) || !allowlist.contains(&symbol) {
                continue;
            }
            let sig = &function.function.dfg.signatures[ext_func.signature];
            let id = module
                .declare_function(&symbol, cranelift_module::Linkage::Import, sig)
                .map_err(|e| JitError::RuntimeKit(format!("failed to declare import '{symbol}': {e}")))?;
            func_ids.insert(symbol, id);
        }
    }
    Ok(())
}

pub(super) fn validate_exact_symbol_references(
    artifact: &CodegenArtifact,
    kit_exports: &HashSet<String>,
    authorized_user_ffi: &HashSet<String>,
    imports: &HashSet<String>,
) -> Result<(), JitError> {
    let defined = artifact.functions.iter().map(|function| function.name.as_str()).collect::<HashSet<_>>();
    for function in &artifact.functions {
        for (_, external) in function.function.dfg.ext_funcs.iter() {
            let cranelift_codegen::ir::ExternalName::TestCase(name) = &external.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw());
            if defined.contains(symbol.as_ref())
                || kit_exports.contains(symbol.as_ref())
                || authorized_user_ffi.contains(symbol.as_ref())
                || imports.contains(symbol.as_ref())
            {
                continue;
            }
            return Err(JitError::RuntimeKit(format!(
                "JIT symbol `{symbol}` is not approved by the exact ABI-v5 runtime kit"
            )));
        }
    }
    Ok(())
}
