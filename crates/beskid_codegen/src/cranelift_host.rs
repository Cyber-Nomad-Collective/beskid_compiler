//! Shared Cranelift module helpers for JIT and AOT backends (builtin imports, extern FFI checks,
//! TestCase name remapping). Object builds use the same extern FFI rules as JIT.

use std::collections::HashMap;
use std::fmt;

use beskid_abi::{AbiParamKind, AbiReturnKind, BUILTIN_SPECS};
use cranelift_codegen::ir::{AbiParam, ExternalName, Signature, UserExternalName, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, FuncOrDataId, Linkage, Module, ModuleError};

use crate::CodegenArtifact;

/// Remapping failures when resolving `ExternalName::TestCase` references against a module.
#[derive(Debug)]
pub enum HostError {
    MissingSymbol(String),
    InvalidGlobalValue,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::MissingSymbol(s) => write!(f, "missing symbol: {s}"),
            HostError::InvalidGlobalValue => write!(f, "expected symbol global value"),
        }
    }
}

impl std::error::Error for HostError {}

/// Failure while declaring extern imports on a Cranelift [`Module`] (signature validation or `declare_function`).
#[derive(Debug)]
pub enum ExternDeclarationError {
    InvalidSignature(String),
    Module(ModuleError),
}

impl fmt::Display for ExternDeclarationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExternDeclarationError::InvalidSignature(msg) => f.write_str(msg),
            ExternDeclarationError::Module(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ExternDeclarationError {}

/// Build a Cranelift [`Signature`] for a Beskid builtin from ABI param/return kinds and the module pointer type.
pub fn builtin_signature(
    pointer: cranelift_codegen::ir::Type,
    call_conv: CallConv,
    params: &[AbiParamKind],
    returns: AbiReturnKind,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    for param in params {
        let ty = match param {
            AbiParamKind::Ptr => pointer,
            AbiParamKind::I64 => types::I64,
        };
        sig.params.push(AbiParam::new(ty));
    }
    match returns {
        AbiReturnKind::Void | AbiReturnKind::Never => {}
        AbiReturnKind::Ptr => sig.returns.push(AbiParam::new(pointer)),
        AbiReturnKind::I64 => sig.returns.push(AbiParam::new(types::I64)),
        AbiReturnKind::I32 => sig.returns.push(AbiParam::new(types::I32)),
    }
    sig
}

/// Declare every entry in [`beskid_abi::BUILTIN_SPECS`] as import functions on `module`.
pub fn declare_builtin_imports<M: Module>(
    module: &mut M,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), ModuleError> {
    let pointer = module.isa().pointer_type();
    let call_conv = module.isa().default_call_conv();

    for spec in BUILTIN_SPECS {
        let signature = builtin_signature(pointer, call_conv, spec.params, spec.returns);
        let id = module.declare_function(spec.symbol, Linkage::Import, &signature)?;
        func_ids.insert(spec.symbol.to_owned(), id);
    }

    Ok(())
}

/// Ensure `sig` uses only pointer-sized integers and a small allowlist of scalar types permitted for extern FFI.
pub fn validate_ffi_signature(
    sig: &Signature,
    pointer: cranelift_codegen::ir::Type,
) -> Result<(), String> {
    let check_ty = |ty: cranelift_codegen::ir::Type| -> bool {
        ty == pointer || ty == types::I64 || ty == types::I32 || ty == types::I8 || ty == types::F64
    };
    for p in &sig.params {
        if !check_ty(p.value_type) {
            return Err(format!("param type {} not allowed", p.value_type));
        }
    }
    for r in &sig.returns {
        if !check_ty(r.value_type) {
            return Err(format!("return type {} not allowed", r.value_type));
        }
    }
    Ok(())
}

/// Scan lowered CLIF for [`ExternalName::TestCase`] callees that match `artifact.extern_imports`, validate FFI, and merge duplicate symbols.
pub fn collect_validated_extern_signatures<M: Module>(
    module: &M,
    artifact: &CodegenArtifact,
) -> Result<HashMap<String, Signature>, String> {
    let pointer = module.isa().pointer_type();
    let mut extern_sigs: HashMap<String, Signature> = HashMap::new();
    let mut ctx_probe = module.make_context();
    for function in &artifact.functions {
        ctx_probe.func = function.function.clone();
        for (_func_ref, ext_func) in ctx_probe.func.dfg.ext_funcs.iter() {
            if let ExternalName::TestCase(name) = &ext_func.name {
                let symbol = String::from_utf8_lossy(name.raw()).to_string();
                if artifact.extern_imports.iter().any(|e| e.symbol == symbol) {
                    let sig = ctx_probe.func.dfg.signatures[ext_func.signature].clone();
                    validate_ffi_signature(&sig, pointer).map_err(|msg| {
                        format!("extern signature not allowed for {symbol}: {msg}")
                    })?;
                    if let Some(prev) = extern_sigs.get(&symbol) {
                        if prev != &sig {
                            return Err(format!(
                                "extern signature mismatch for {symbol} across callsites"
                            ));
                        }
                    } else {
                        extern_sigs.insert(symbol, sig);
                    }
                }
            }
        }
        module.clear_context(&mut ctx_probe);
    }
    Ok(extern_sigs)
}

/// Declare each [`CodegenArtifact`] function on `module` with the given [`Linkage`], recording ids in `func_ids`.
pub fn declare_user_functions<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    linkage: Linkage,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<Vec<String>, ModuleError> {
    declare_user_functions_with_link_symbols(module, artifact, linkage, func_ids, |name| {
        name.to_string()
    })
}

/// Like [`declare_user_functions`], but allows renaming symbols at the object boundary (AOT `Main` → `main`).
pub fn declare_user_functions_with_link_symbols<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    linkage: Linkage,
    func_ids: &mut HashMap<String, FuncId>,
    link_symbol: impl Fn(&str) -> String,
) -> Result<Vec<String>, ModuleError> {
    let mut declared = Vec::with_capacity(artifact.functions.len());
    for function in &artifact.functions {
        let emitted_symbol = link_symbol(&function.name);
        let func_id =
            module.declare_function(&emitted_symbol, linkage, &function.function.signature)?;
        func_ids.insert(function.name.clone(), func_id);
        declared.push(emitted_symbol);
    }
    Ok(declared)
}

/// After [`collect_validated_extern_signatures`], declare each extern as an import on `module`.
pub fn declare_validated_extern_imports<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), ExternDeclarationError> {
    let extern_sigs = collect_validated_extern_signatures(module, artifact)
        .map_err(ExternDeclarationError::InvalidSignature)?;
    for (symbol, sig) in &extern_sigs {
        let id = module
            .declare_function(symbol, Linkage::Import, sig)
            .map_err(ExternDeclarationError::Module)?;
        func_ids.insert(symbol.clone(), id);
    }
    Ok(())
}

/// Rewrite `ExternalName::TestCase` function references and symbol globals to [`ExternalName::user`] using `func_ids` / module names.
pub fn remap_testcase_externals<M: Module>(
    module: &M,
    ctx: &mut cranelift_codegen::Context,
    func_ids: &HashMap<String, FuncId>,
) -> Result<(), HostError> {
    let mut func_remaps = Vec::new();
    for (func_ref, ext_func) in ctx.func.dfg.ext_funcs.iter() {
        let ExternalName::TestCase(name) = &ext_func.name else {
            continue;
        };
        let symbol = String::from_utf8_lossy(name.raw()).to_string();
        func_remaps.push((func_ref, symbol));
    }
    for (func_ref, symbol) in func_remaps {
        let func_id = func_ids
            .get(&symbol)
            .copied()
            .ok_or_else(|| HostError::MissingSymbol(symbol.clone()))?;
        let user_ref = ctx.func.declare_imported_user_function(UserExternalName {
            namespace: 0,
            index: func_id.as_u32(),
        });
        ctx.func.dfg.ext_funcs[func_ref].name = ExternalName::user(user_ref);
    }

    let mut data_remaps = Vec::new();
    for (gv, data) in ctx.func.global_values.iter() {
        let cranelift_codegen::ir::GlobalValueData::Symbol { name, .. } = data else {
            continue;
        };
        let ExternalName::TestCase(test_name) = name else {
            continue;
        };
        let symbol = String::from_utf8_lossy(test_name.raw()).to_string();
        data_remaps.push((gv, symbol));
    }
    for (gv, symbol) in data_remaps {
        let id = module
            .get_name(&symbol)
            .ok_or_else(|| HostError::MissingSymbol(symbol.clone()))?;
        let FuncOrDataId::Data(data_id) = id else {
            return Err(HostError::MissingSymbol(symbol));
        };
        let user_ref = ctx.func.declare_imported_user_function(UserExternalName {
            namespace: 1,
            index: data_id.as_u32(),
        });
        let cranelift_codegen::ir::GlobalValueData::Symbol { name, .. } =
            &mut ctx.func.global_values[gv]
        else {
            return Err(HostError::InvalidGlobalValue);
        };
        *name = ExternalName::user(user_ref);
    }
    Ok(())
}
