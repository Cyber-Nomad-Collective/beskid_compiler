use std::collections::{HashMap, HashSet};

use crate::runtime_kit::JitRuntimeKit;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_codegen::cranelift_host::{declare_user_functions, declare_validated_extern_imports, remap_testcase_externals};
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use beskid_pipeline::{
    PipelineObserver, emit_work_unit, observe_phase_result,
    phases::{JIT_EMIT, JIT_FINALIZE},
};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use super::declare::{declare_exact_runtime_imports, declare_import_allowlist_symbols, validate_exact_symbol_references};
use super::errors::JitError;
use super::isa::new_builder;

/// Thin wrapper over [`JITModule`] with Beskid symbol registration and compile/finalize helpers.
pub struct BeskidJitModule {
    module: JITModule,
    func_ids: HashMap<String, FuncId>,
    runtime_kit: JitRuntimeKit,
    kit_exports: HashSet<String>,
    authorized_user_ffi: HashSet<String>,
    import_allowlist: HashSet<String>,
}

impl BeskidJitModule {
    /// JIT module backed only by an exact shared ABI-v5 runtime kit.
    pub fn new_with_runtime_kit(
        prefix: &std::path::Path,
        target: &TargetMetadata,
        profile: RuntimeKitProfile,
        authorized_user_ffi: &[(String, *const u8)],
    ) -> Result<Self, JitError> {
        let runtime = JitRuntimeKit::load(prefix, target, profile).map_err(JitError::RuntimeKit)?;
        if let Some((name, _)) =
            authorized_user_ffi.iter().find(|(name, _)| runtime.metadata().export_allowlist.contains(name))
        {
            return Err(JitError::RuntimeKit(format!(
                "external symbol `{name}` cannot override an ABI-v5 runtime export"
            )));
        }
        let kit_exports: HashSet<String> = runtime.symbols().iter().map(|(name, _)| name.clone()).collect();
        let authorized_user_ffi_names = authorized_user_ffi.iter().map(|(name, _)| name.clone()).collect();
        let mut symbols = runtime.symbols().to_vec();
        symbols.extend_from_slice(authorized_user_ffi);
        let import_allowlist: HashSet<String> = runtime.metadata().import_allowlist.iter().cloned().collect();
        let builder = new_builder(&symbols)?;
        Ok(Self {
            module: JITModule::new(builder),
            func_ids: HashMap::new(),
            runtime_kit: runtime,
            kit_exports,
            authorized_user_ffi: authorized_user_ffi_names,
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
        validate_exact_symbol_references(
            artifact,
            &self.kit_exports,
            &self.authorized_user_ffi,
            &self.import_allowlist,
        )?;

        declare_user_functions(&mut self.module, artifact, Linkage::Local, &mut self.func_ids)?;
        declare_exact_runtime_imports(&mut self.module, artifact, &self.kit_exports, &mut self.func_ids)?;
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
        self.kit_exports.contains(symbol)
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
