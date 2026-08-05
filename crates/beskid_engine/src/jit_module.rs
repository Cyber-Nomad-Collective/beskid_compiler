use std::collections::{HashMap, HashSet};

use crate::runtime_kit::JitRuntimeKit;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_abi::{all_builtin_specs, is_dispatch_symbol};
use beskid_codegen::cranelift_host::{
    ExternDeclarationError, HostError, declare_builtin_imports, declare_user_functions,
    declare_validated_extern_imports, remap_testcase_externals,
};
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use beskid_pipeline::{
    PipelineObserver, emit_work_unit, observe_phase_result,
    phases::{JIT_EMIT, JIT_FINALIZE},
};
use cranelift_codegen::{ir::ExternalName, settings};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, ModuleError, default_libcall_names};
use std::fmt;

/// Failure to build the ISA, declare/define Cranelift module objects, or resolve a symbol name.
#[derive(Debug)]
pub enum JitError {
    Isa(String),
    Module(Box<ModuleError>),
    MissingFunction(String),
    RuntimeKit(String),
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitError::Isa(msg) => write!(f, "{msg}"),
            JitError::Module(err) => write!(f, "{err}"),
            JitError::MissingFunction(name) => write!(f, "missing function `{name}`"),
            JitError::RuntimeKit(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for JitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JitError::Module(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<ModuleError> for JitError {
    fn from(error: ModuleError) -> Self {
        Self::Module(Box::new(error))
    }
}

impl From<HostError> for JitError {
    fn from(value: HostError) -> Self {
        match value {
            HostError::MissingSymbol(name) => JitError::MissingFunction(name),
            HostError::InvalidGlobalValue => JitError::Isa("invalid global value for extern data symbol".to_owned()),
        }
    }
}

impl From<ExternDeclarationError> for JitError {
    fn from(value: ExternDeclarationError) -> Self {
        match value {
            ExternDeclarationError::InvalidSignature(message) => JitError::Isa(message),
            ExternDeclarationError::Module(error) => JitError::Module(Box::new(error)),
        }
    }
}

/// Thin wrapper over [`JITModule`] with Beskid symbol registration and compile/finalize helpers.
pub struct BeskidJitModule {
    module: JITModule,
    func_ids: HashMap<String, FuncId>,
    builtins_declared: bool,
    runtime_kit: JitRuntimeKit,
    exact_symbols: HashSet<String>,
    import_allowlist: HashSet<String>,
}

impl BeskidJitModule {
    /// JIT module backed only by an exact shared ABI-v5 runtime kit.
    pub fn new_with_runtime_kit(
        prefix: &std::path::Path,
        target: &TargetMetadata,
        profile: RuntimeKitProfile,
        extras: &[(String, *const u8)],
    ) -> Result<Self, JitError> {
        let runtime = JitRuntimeKit::load(prefix, target, profile).map_err(JitError::RuntimeKit)?;
        if let Some((name, _)) = extras.iter().find(|(name, _)| runtime.metadata().export_allowlist.contains(name)) {
            return Err(JitError::RuntimeKit(format!(
                "external symbol `{name}` cannot override an ABI-v5 runtime export"
            )));
        }
        let mut symbols = runtime.symbols().to_vec();
        symbols.extend_from_slice(extras);
        // Soft builtins are process-linked (`beskid_runtime`), not ABI-v5 kit exports.
        // Validation already allowlists them; Cranelift still needs concrete addresses.
        let exact_symbols: HashSet<String> = symbols.iter().map(|(name, _)| name.clone()).collect();
        let import_allowlist: HashSet<String> = runtime.metadata().import_allowlist.iter().cloned().collect();
        for (name, addr) in process_linked_soft_builtins() {
            if exact_symbols.contains(&name) {
                continue;
            }
            symbols.push((name, addr));
        }
        let builder = new_builder(&symbols)?;
        Ok(Self {
            module: JITModule::new(builder),
            func_ids: HashMap::new(),
            builtins_declared: false,
            runtime_kit: runtime,
            exact_symbols,
            import_allowlist,
        })
    }

    /// Exact ABI-v5 runtime kit backing this module's imported runtime symbols.
    pub(crate) fn runtime_kit(&self) -> &JitRuntimeKit {
        &self.runtime_kit
    }

    /// Declare builtins (once), user funcs, externs, data, define bodies, finalize definitions.
    pub fn compile(&mut self, artifact: &CodegenArtifact) -> Result<(), JitError> {
        self.compile_with_pipeline(artifact, None)
    }

    /// Same as [`Self::compile`], reporting per-function emit progress when `pipeline` is set.
    pub fn compile_with_pipeline(
        &mut self,
        artifact: &CodegenArtifact,
        pipeline: Option<&dyn PipelineObserver>,
    ) -> Result<(), JitError> {
        validate_exact_symbol_references(artifact, &self.exact_symbols, &self.import_allowlist)?;
        if !self.builtins_declared {
            declare_builtin_imports(&mut self.module, &mut self.func_ids)?;
            self.builtins_declared = true;
        }

        declare_user_functions(&mut self.module, artifact, Linkage::Local, &mut self.func_ids)?;
        declare_exact_runtime_imports(&mut self.module, artifact, &self.exact_symbols, &mut self.func_ids)?;
        declare_validated_extern_imports(&mut self.module, artifact, &mut self.func_ids)?;
        declare_import_allowlist_symbols(&mut self.module, artifact, &self.import_allowlist, &mut self.func_ids)?;

        emit_string_literals(&mut self.module, artifact)?;
        emit_type_descriptors(&mut self.module, artifact)?;
        beskid_codegen::emit_closure_static_plans(&mut self.module, artifact)?;

        let mut ctx = self.module.make_context();
        let total = artifact.functions.len() as u64;
        for (index, function) in artifact.functions.iter().enumerate() {
            let func_id = self
                .func_ids
                .get(&function.name)
                .copied()
                .ok_or_else(|| JitError::MissingFunction(function.name.clone()))?;
            ctx.func = function.function.clone();
            remap_testcase_externals(&self.module, &mut ctx, &self.func_ids)?;
            self.module.define_function(func_id, &mut ctx)?;
            self.module.clear_context(&mut ctx);
            emit_work_unit(pipeline, JIT_EMIT, (index as u64) + 1, total, function.name.clone());
        }

        observe_phase_result(pipeline, JIT_FINALIZE, || self.module.finalize_definitions().map_err(JitError::from))?;
        Ok(())
    }

    /// [`FuncId`] for a declared function or import symbol name, if present.
    pub fn get_func_id(&self, name: &str) -> Option<FuncId> {
        self.func_ids.get(name).copied()
    }

    /// True only for an address loaded from this exact validated runtime kit.
    pub fn is_exact_runtime_symbol(&self, symbol: &str) -> bool {
        self.exact_symbols.contains(symbol)
    }

    /// Executable address after [`JITModule::finalize_definitions`]; undefined if not finalized.
    ///
    /// # Safety
    ///
    /// The caller must ensure `func_id` belongs to this finalized module and cast the returned
    /// pointer to the exact generated function signature before calling it.
    pub unsafe fn get_finalized_function_ptr(&mut self, func_id: FuncId) -> *const u8 {
        self.module.get_finalized_function(func_id)
    }

    /// Access the underlying Cranelift JIT module (tests / advanced linking).
    pub fn module(&mut self) -> &mut JITModule {
        &mut self.module
    }
}

fn declare_exact_runtime_imports(
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
fn declare_import_allowlist_symbols(
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

fn validate_exact_symbol_references(artifact: &CodegenArtifact, approved: &HashSet<String>, imports: &HashSet<String>) -> Result<(), JitError> {
    let defined = artifact.functions.iter().map(|function| function.name.as_str()).collect::<HashSet<_>>();
    for function in &artifact.functions {
        for (_, external) in function.function.dfg.ext_funcs.iter() {
            let cranelift_codegen::ir::ExternalName::TestCase(name) = &external.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw());
            // Soft builtins (`interop_dispatch_*`, `panic_str`, syscalls, …) are declared via
            // `declare_builtin_imports` from process-linked `beskid_runtime`, not the ABI-v5 kit
            // export allowlist. Match AOT `linking::validate::is_runtime_builtin`.
            if defined.contains(symbol.as_ref())
                || approved.contains(symbol.as_ref())
                || imports.contains(symbol.as_ref())
                || is_runtime_builtin(symbol.as_ref())
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

fn is_runtime_builtin(symbol: &str) -> bool {
    all_builtin_specs().any(|spec| spec.symbol == symbol) || is_dispatch_symbol(symbol)
}

/// Addresses for soft builtins declared by [`declare_builtin_imports`].
///
/// Exact ABI-v5 kit dylibs export only `beskid_rt_v5_*` symbols. Soft builtins such as
/// `interop_dispatch_*` / `panic_str` live in process-linked `beskid_runtime` and must be
/// registered on the JIT builder or Cranelift fails with `can't resolve symbol`.
fn process_linked_soft_builtins() -> Vec<(String, *const u8)> {
    // Keep this process-linked compatibility list in sync with `BUILTIN_SPECS`.
    vec![
        ("alloc".into(), beskid_runtime::alloc as *const u8),
        ("args_count".into(), beskid_runtime::args_count as *const u8),
        ("args_get".into(), beskid_runtime::args_get as *const u8),
        ("beskid_register_callbacks".into(), beskid_runtime::beskid_register_callbacks as *const u8),
        ("beskid_register_handlers".into(), beskid_runtime::beskid_register_handlers as *const u8),
        ("beskid_runtime_abi_version".into(), beskid_runtime::beskid_runtime_abi_version as *const u8),
        ("composition_bind_plural".into(), beskid_runtime::composition_bind_plural as *const u8),
        ("composition_container_create".into(), beskid_runtime::composition_container_create as *const u8),
        ("composition_container_drop".into(), beskid_runtime::composition_container_drop as *const u8),
        ("composition_launch".into(), beskid_runtime::composition_launch as *const u8),
        ("composition_register".into(), beskid_runtime::composition_register as *const u8),
        ("composition_resolve".into(), beskid_runtime::composition_resolve as *const u8),
        ("composition_resolve_plural".into(), beskid_runtime::composition_resolve_plural as *const u8),
        ("composition_scope_depth".into(), beskid_runtime::composition_scope_depth as *const u8),
        ("composition_scope_enter".into(), beskid_runtime::composition_scope_enter as *const u8),
        ("composition_scope_leave".into(), beskid_runtime::composition_scope_leave as *const u8),
        ("composition_shutdown".into(), beskid_runtime::composition_shutdown as *const u8),
        ("dynamic_cast_checked".into(), beskid_runtime::dynamic_cast_checked as *const u8),
        ("dynamic_cell_create".into(), beskid_runtime::dynamic_cell_create as *const u8),
        ("dynamic_cell_wrap".into(), beskid_runtime::dynamic_cell_wrap as *const u8),
        ("dynamic_map_aot".into(), beskid_runtime::dynamic_map_aot as *const u8),
        ("dynamic_map_fallback".into(), beskid_runtime::dynamic_map_fallback as *const u8),
        ("dynamic_object_alloc".into(), beskid_runtime::dynamic_object_alloc as *const u8),
        ("fiber_yield".into(), beskid_runtime::fiber_yield as *const u8),
        ("gc_register_root".into(), beskid_runtime::gc_register_root as *const u8),
        ("gc_root_handle".into(), beskid_runtime::gc_root_handle as *const u8),
        ("gc_unregister_root".into(), beskid_runtime::gc_unregister_root as *const u8),
        ("gc_unroot_handle".into(), beskid_runtime::gc_unroot_handle as *const u8),
        ("gc_write_barrier".into(), beskid_runtime::gc_write_barrier as *const u8),
        ("math_ceil".into(), beskid_runtime::math_ceil as *const u8),
        ("math_floor".into(), beskid_runtime::math_floor as *const u8),
        ("math_log".into(), beskid_runtime::math_log as *const u8),
        ("math_sqrt".into(), beskid_runtime::math_sqrt as *const u8),
        ("interop_dispatch_ptr".into(), beskid_runtime::interop_dispatch_ptr as *const u8),
        ("interop_dispatch_unit".into(), beskid_runtime::interop_dispatch_unit as *const u8),
        ("interop_dispatch_usize".into(), beskid_runtime::interop_dispatch_usize as *const u8),
        ("interop_dispatch_i64".into(), beskid_runtime::interop_dispatch_i64 as *const u8),
        ("panic".into(), beskid_runtime::panic as *const u8),
        ("panic_str".into(), beskid_runtime::panic_str as *const u8),
        ("syscall_read".into(), beskid_runtime::builtins::syscall_read as *const u8),
        ("syscall_read_bytes".into(), beskid_runtime::builtins::syscall_read_bytes as *const u8),
        ("syscall_write".into(), beskid_runtime::syscall_write as *const u8),
        ("syscall_write_bytes".into(), beskid_runtime::builtins::syscall_write_bytes as *const u8),
        ("runtime_preempt_check".into(), beskid_runtime::runtime_preempt_check as *const u8),
    ]
}

fn new_builder(extras: &[(String, *const u8)]) -> Result<JITBuilder, JitError> {
    let isa_builder = cranelift_native::builder().map_err(|err| JitError::Isa(err.to_string()))?;
    let isa =
        isa_builder.finish(settings::Flags::new(settings::builder())).map_err(|err| JitError::Isa(err.to_string()))?;
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    for (sym, addr) in extras {
        builder.symbol(sym, *addr);
    }
    Ok(builder)
}
