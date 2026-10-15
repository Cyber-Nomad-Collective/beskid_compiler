//! Shared Cranelift module helpers for JIT and AOT backends (builtin imports, extern FFI checks,
//! TestCase name remapping). Object builds use the same extern FFI rules as JIT.

use std::collections::HashMap;
use std::fmt;

use beskid_abi::abi_v5::AbiType;
use beskid_abi::interop::c_profile::C_PROFILE_PERMITTED_SCALARS;
use beskid_abi::runtime_source::{CorelibServiceAbi, CorelibServiceAbiType};
use cranelift_codegen::ir::{AbiParam, ExternalName, Signature, UserExternalName, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_module::{FuncId, FuncOrDataId, Linkage, Module, ModuleError};

use crate::CodegenArtifact;

/// Construct the shared settings builder for every production JIT and AOT ISA.
///
/// Cranelift's x64 tail-call emitter currently requires frame pointers. Keeping
/// this invariant here prevents lowering and final machine emission from using
/// different ISA contracts.
///
/// `enable_probestack`/`probestack_strategy` guard a single function's own stack
/// frame: Cranelift defaults (`cranelift-codegen` 0.136.0, `src/settings.rs`) are
/// `enable_probestack = false`, `probestack_strategy = "outline"`,
/// `probestack_size_log2 = 12` (4 KiB, matching the OS guard-page granularity).
/// Left at the default, a single frame larger than one guard page can `sub rsp`
/// past the guard region in one instruction without ever touching it (a "stack
/// clash"), landing in genuinely unmapped memory instead of faulting cleanly on
/// the guard page -- this is a real gap for any Beskid function whose own locals
/// exceed 4 KiB, independent of recursion depth. `probestack_strategy = "outline"`
/// would instead emit a call to `ExternalName::LibCall(LibCall::Probestack)`,
/// which both the JIT (`cranelift_jit` libcall symbol resolution) and the AOT
/// object/link path (`crates/beskid_aot`) would then have to resolve to a real
/// `__cranelift_probestack`-equivalent host export that does not exist anywhere
/// in this runtime today -- introducing that symbol, and keeping the JIT and AOT
/// resolution paths in sync for it, is its own surface. `"inline"` instead emits
/// the probe directly in the function's own prologue (`gen_inline_probestack`,
/// `cranelift-codegen` `src/isa/x64/abi.rs`): a store to each guard-sized page
/// from the frame's top down to `rsp`, unrolled for small frames (<= 4 probes)
/// or a loop for larger ones (`stack_probe_loop`), with no runtime symbol at all.
/// Picked here for that reason -- self-contained, no new host export, no
/// JIT/AOT resolution to keep in sync.
///
/// NOTE: this does not, by itself, bound *cumulative* recursion depth (many
/// small frames that individually stay under one guard page never trigger a
/// probe at all) -- it only guarantees a single oversized frame cannot skip the
/// guard page. Cranelift's separate wasm-style `Function::stack_limit` /
/// cumulative stack-check mechanism (`machinst/abi.rs`, `insert_stack_check`) is
/// never set by `beskid_isle`/`beskid_codegen` (confirmed: no
/// `stack_limit`/`ir::Function::stack_limit` references anywhere under
/// `crates/beskid_isle`, `crates/beskid_codegen`, `crates/beskid_engine`,
/// `crates/beskid_aot`), so deep recursion of small frames is still bounded only
/// by whatever guard page exists on the stack actually in use -- this codebase's
/// own growable guard-page mechanism (`GuardedStackAllocate` /
/// `runtime/beskid/src/Runtime/Fiber/Scheduler/Context.bd`) only covers spawned
/// fiber stacks, not a plain native-thread call into JIT code (see
/// `crates/beskid_engine/tests/native_kit/heap_growth_native.rs::run_heap_fixture`, which
/// calls the JIT entrypoint directly on the calling thread). Whether that
/// distinction is the actual cause of the `deep_recursion` SIGSEGV, or whether
/// something is separately corrupting a return address before any guard page is
/// reached, is exactly what still needs gdb evidence (unavailable while the
/// remote builder is offline) to settle -- this probestack fix closes a real,
/// independently-justified gap, but is not asserted here to be the fix for that
/// specific crash.
pub fn production_isa_settings_builder() -> Result<settings::Builder, settings::SetError> {
    let mut builder = settings::builder();
    builder.set("preserve_frame_pointers", "true")?;
    builder.set("enable_probestack", "true")?;
    builder.set("probestack_strategy", "inline")?;
    Ok(builder)
}

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

/// Cranelift [`Signature`] of one canonical ABI-v5 runtime service shape. `None` when a slot has
/// no scalar register class.
pub fn corelib_service_signature(
    pointer: cranelift_codegen::ir::Type,
    call_conv: CallConv,
    abi: &CorelibServiceAbi,
) -> Option<Signature> {
    let scalar = |ty: CorelibServiceAbiType| match ty {
        CorelibServiceAbiType::Pointer | CorelibServiceAbiType::String | CorelibServiceAbiType::Usize => Some(pointer),
        CorelibServiceAbiType::U8 => Some(types::I8),
        CorelibServiceAbiType::I32 | CorelibServiceAbiType::U32 => Some(types::I32),
        CorelibServiceAbiType::I64 | CorelibServiceAbiType::U64 => Some(types::I64),
        CorelibServiceAbiType::F32 => Some(types::F32),
        CorelibServiceAbiType::F64 => Some(types::F64),
        CorelibServiceAbiType::Void | CorelibServiceAbiType::Never => None,
    };
    let mut signature = Signature::new(call_conv);
    for ty in &abi.parameters {
        signature.params.push(AbiParam::new(scalar(*ty)?));
    }
    if !matches!(abi.result, CorelibServiceAbiType::Void | CorelibServiceAbiType::Never) {
        signature.returns.push(AbiParam::new(scalar(abi.result)?));
    }
    Some(signature)
}

/// Canonical import signature of a process-linked runtime symbol ([`beskid_abi::is_runtime_import_symbol`]).
/// The shape comes only from the generated ABI-v5 tables
/// (`canonical_corelib_service_abi_for_adapter`); `None` for any other symbol or an ambiguous shape.
pub fn runtime_import_signature(pointer: cranelift_codegen::ir::Type, call_conv: CallConv, symbol: &str) -> Option<Signature> {
    if !beskid_abi::is_runtime_import_symbol(symbol) {
        return None;
    }
    let abi = beskid_abi::runtime_source::canonical_corelib_service_abi_for_adapter(symbol)?;
    corelib_service_signature(pointer, call_conv, &abi)
}

/// Declare only runtime imports referenced by the artifact's lowered CLIF, in symbol order, each
/// with its canonical ABI-v5 signature ([`runtime_import_signature`]).
///
/// AOT archives retain every declared import, even when no emitted function calls it. Restrict
/// the object boundary to actual `TestCase` callees so a canonical runtime archive cannot leak
/// unrelated runtime symbols.
pub fn declare_referenced_builtin_imports<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), ModuleError> {
    let mut referenced = artifact
        .functions
        .iter()
        .flat_map(|function| function.function.dfg.ext_funcs.iter())
        .filter_map(|(_func_ref, ext_func)| match &ext_func.name {
            ExternalName::TestCase(name) => Some(String::from_utf8_lossy(name.raw()).to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    referenced.sort_unstable();
    referenced.dedup();
    let pointer = module.isa().pointer_type();
    let call_conv = module.isa().default_call_conv();

    for symbol in referenced {
        if func_ids.contains_key(&symbol) {
            continue;
        }
        let Some(signature) = runtime_import_signature(pointer, call_conv, &symbol) else {
            continue;
        };
        let id = module.declare_function(&symbol, Linkage::Import, &signature)?;
        func_ids.insert(symbol, id);
    }

    Ok(())
}

/// Ensure `sig` uses only types permitted for extern FFI by the `Interop.Contracts` C ABI
/// profile. The allowed scalar CLIF types are derived from
/// [`C_PROFILE_PERMITTED_SCALARS`] (defense-in-depth: the type checker already
/// validates extern signatures via [`beskid_abi::interop::c_profile::CAbiProfile`]).
/// View records (`CStringView`, `CBuffer`, `CArrayView`) lower to `pointer` +
/// `i64` pairs, both of which are in the permitted set.
pub fn validate_ffi_signature(sig: &Signature, pointer: cranelift_codegen::ir::Type) -> Result<(), String> {
    let check_ty = |ty: cranelift_codegen::ir::Type| -> bool { ty == pointer || is_permited_ffi_scalar(ty) };
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

/// Map a CLIF type to its source [`AbiType`], if any.
fn clif_type_to_abi_type(ty: cranelift_codegen::ir::Type) -> Option<AbiType> {
    use cranelift_codegen::ir::types;
    if ty == types::I8 {
        Some(AbiType::I8)
    } else if ty == types::I16 {
        Some(AbiType::I16)
    } else if ty == types::I32 {
        Some(AbiType::I32)
    } else if ty == types::I64 {
        Some(AbiType::I64)
    } else if ty == types::F32 {
        Some(AbiType::F32)
    } else if ty == types::F64 {
        Some(AbiType::F64)
    } else {
        None
    }
}

/// Returns `true` when `ty` is a CLIF scalar permitted at the FFI boundary by
/// the C ABI profile.
fn is_permited_ffi_scalar(ty: cranelift_codegen::ir::Type) -> bool {
    let Some(abi_type) = clif_type_to_abi_type(ty) else {
        return false;
    };
    C_PROFILE_PERMITTED_SCALARS.contains(&abi_type)
}

/// Scan lowered CLIF for [`ExternalName::TestCase`] callees that match `artifact.extern_imports`, validate FFI, and merge duplicate symbols.
pub fn collect_validated_extern_signatures<M: Module>(
    module: &M,
    artifact: &CodegenArtifact,
) -> Result<HashMap<String, Signature>, String> {
    let pointer = module.isa().pointer_type();
    let mut extern_sigs: HashMap<String, Signature> = HashMap::new();
    for function in &artifact.functions {
        for (_func_ref, ext_func) in function.function.dfg.ext_funcs.iter() {
            if let ExternalName::TestCase(name) = &ext_func.name {
                let symbol = String::from_utf8_lossy(name.raw()).to_string();
                if artifact
                    .extern_imports
                    .iter()
                    .chain(artifact.trusted_extern_imports.iter())
                    .any(|e| e.symbol == symbol)
                {
                    let sig = function.function.dfg.signatures[ext_func.signature].clone();
                    validate_ffi_signature(&sig, pointer)
                        .map_err(|msg| format!("extern signature not allowed for {symbol}: {msg}"))?;
                    if let Some(prev) = extern_sigs.get(&symbol) {
                        if prev != &sig {
                            return Err(format!("extern signature mismatch for {symbol} across callsites"));
                        }
                    } else {
                        extern_sigs.insert(symbol, sig);
                    }
                }
            }
        }
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
    declare_user_functions_with_link_symbols(module, artifact, linkage, func_ids, |name| name.to_string())
}

/// Like [`declare_user_functions`], but allows renaming symbols at the object boundary (AOT `Main` → `main`).
pub fn declare_user_functions_with_link_symbols<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    linkage: Linkage,
    func_ids: &mut HashMap<String, FuncId>,
    link_symbol: impl Fn(&str) -> String,
) -> Result<Vec<String>, ModuleError> {
    declare_user_functions_with_link_symbols_and_linkage(module, artifact, func_ids, link_symbol, |_| linkage)
}

/// Like [`declare_user_functions_with_link_symbols`], but chooses linkage per emitted object
/// symbol. AOT runtime publication uses this to keep syntax implementation functions local while
/// exporting only manifest-approved ABI functions.
pub fn declare_user_functions_with_link_symbols_and_linkage<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
    func_ids: &mut HashMap<String, FuncId>,
    link_symbol: impl Fn(&str) -> String,
    linkage_for_symbol: impl Fn(&str) -> Linkage,
) -> Result<Vec<String>, ModuleError> {
    let mut declared = Vec::with_capacity(artifact.functions.len());
    for function in &artifact.functions {
        let emitted_symbol = link_symbol(&function.name);
        let linkage = linkage_for_symbol(&emitted_symbol);
        // `link_symbol` (ordinarily `beskid_codegen::object_link_symbol`) is meant to resolve
        // every name to either an explicit `[Export(Symbol:"...")]` identity or a mangled,
        // link-name-safe internal identity -- never to a raw internal lowering key. A name that
        // still fails the C-identifier / mangling alphabet here (most notably one still carrying
        // a `#`-delimited trace id or other lowering-internal bookkeeping fragment, e.g.
        // `#gN:nM`) must not be handed to the linker as a symbol other code can actually resolve
        // against: fail closed instead of silently declaring an unlinkable or unstable exported
        // symbol.
        //
        // `Linkage::Local` is exempt: a purely local Cranelift/JIT symbol never leaves this
        // module's own bookkeeping (`beskid_engine`'s JIT path declares every function -- callee
        // or not -- with a fixed `Linkage::Local` and an identity `link_symbol`, then always
        // looks functions back up by `function.name` through its own `func_ids` map, never by the
        // declared Cranelift symbol string), so a `#`-carrying local name here is inert rather
        // than a real linked/exported symbol reaching an object file or an external consumer.
        if linkage != Linkage::Local && !crate::artifact::is_valid_link_name(&emitted_symbol) {
            return Err(ModuleError::Backend(anyhow::anyhow!(
                "item `{}` produced an invalid link name `{emitted_symbol}` (contains a character \
                 that is not valid in a C identifier or the mangling scheme)",
                function.name
            )));
        }
        let func_id = module.declare_function(&emitted_symbol, linkage, &function.function.signature)?;
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
    let extern_sigs =
        collect_validated_extern_signatures(module, artifact).map_err(ExternDeclarationError::InvalidSignature)?;
    // Declare in symbol order: declaration order is the object's symbol-table order.
    let mut extern_sigs = extern_sigs.iter().collect::<Vec<_>>();
    extern_sigs.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (symbol, sig) in extern_sigs {
        if func_ids.contains_key(symbol) {
            continue;
        }
        // An optional `[Extern]` symbol may be absent: declare it preemptible, which a JIT
        // resolves to null when no address is supplied and an object file records as a weak
        // undefined reference. Generated code reaches it only through a null-checking thunk.
        let optional = artifact.extern_imports.iter().any(|import| import.optional && &import.symbol == symbol);
        let linkage = if optional { Linkage::Preemptible } else { Linkage::Import };
        let id = module.declare_function(symbol, linkage, sig).map_err(ExternDeclarationError::Module)?;
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
        let func_id = func_ids.get(&symbol).copied().ok_or_else(|| HostError::MissingSymbol(symbol.clone()))?;
        let user_ref =
            ctx.func.declare_imported_user_function(UserExternalName { namespace: 0, index: func_id.as_u32() });
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
        let id = module.get_name(&symbol).ok_or_else(|| HostError::MissingSymbol(symbol.clone()))?;
        let FuncOrDataId::Data(data_id) = id else {
            return Err(HostError::MissingSymbol(symbol));
        };
        let user_ref =
            ctx.func.declare_imported_user_function(UserExternalName { namespace: 1, index: data_id.as_u32() });
        let cranelift_codegen::ir::GlobalValueData::Symbol { name, .. } = &mut ctx.func.global_values[gv] else {
            return Err(HostError::InvalidGlobalValue);
        };
        *name = ExternalName::user(user_ref);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cranelift_codegen::ir::Function;
    use cranelift_codegen::isa;
    use cranelift_codegen::settings;
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, default_libcall_names};

    use super::declare_user_functions_with_link_symbols_and_linkage;
    use crate::{CodegenArtifact, LoweredFunction};

    fn jit_module() -> JITModule {
        let isa = isa::lookup_by_name("x86_64")
            .expect("x86_64 target supported")
            .finish(settings::Flags::new(settings::builder()))
            .expect("isa build");
        JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()))
    }

    /// A `link_symbol` closure that never resolves an internal lowering key to a clean export
    /// name (as `object_link_symbol` would for a matched `[Export(Symbol:"...")]`) must not be
    /// allowed to declare that raw key as the object's actual link name. This is exactly the
    /// shape `object_link_symbol` produces today for a specialized generic or synthesized item
    /// that has no matching export entry: it falls through and returns the lowering-internal
    /// name (`Item#generic_1_2`, or an internal trace id fragment such as `#gN:nM`) unchanged.
    #[test]
    fn rejects_a_final_link_name_still_carrying_an_internal_trace_fragment() {
        let mut module = jit_module();
        let artifact = CodegenArtifact {
            functions: vec![LoweredFunction { name: "Widget#gen1:nod2".into(), function: Function::new() }],
            ..CodegenArtifact::default()
        };
        let mut func_ids = HashMap::new();

        let result = declare_user_functions_with_link_symbols_and_linkage(
            &mut module,
            &artifact,
            &mut func_ids,
            |name| name.to_owned(),
            |_| Linkage::Export,
        );

        let error = result.expect_err("a `#`-carrying link name must be rejected, not declared");
        let message = error.to_string();
        assert!(message.contains("Widget#gen1:nod2"), "error should name the offending link name: {message}");
        assert!(func_ids.is_empty(), "no function should be declared once its link name is rejected");
    }

    /// An ordinary, already-mangled link name (the common case: a plain source name or an
    /// `object_link_symbol` match against an explicit `[Export(Symbol:"...")]` entry) must still
    /// declare successfully -- the new check must not reject legitimate names.
    #[test]
    fn accepts_an_ordinary_c_identifier_link_name() {
        let mut module = jit_module();
        let artifact = CodegenArtifact {
            functions: vec![LoweredFunction { name: "beskid_widget_construct".into(), function: Function::new() }],
            ..CodegenArtifact::default()
        };
        let mut func_ids = HashMap::new();

        let declared = declare_user_functions_with_link_symbols_and_linkage(
            &mut module,
            &artifact,
            &mut func_ids,
            |name| name.to_owned(),
            |_| Linkage::Export,
        )
        .expect("an ordinary C-identifier link name must be accepted");

        assert_eq!(declared, vec!["beskid_widget_construct".to_owned()]);
        assert!(func_ids.contains_key("beskid_widget_construct"));
    }
}
