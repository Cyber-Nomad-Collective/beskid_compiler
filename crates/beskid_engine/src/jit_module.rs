use std::collections::HashMap;

use beskid_abi::{
    SYM_ALLOC, SYM_ARRAY_LEN, SYM_ARRAY_NEW, SYM_CHANNEL_CLOSE, SYM_CHANNEL_CREATE,
    SYM_CHANNEL_RECEIVE, SYM_CHANNEL_RECEIVE_VALUE, SYM_CHANNEL_SEND, SYM_CHANNEL_TRY_RECEIVE,
    SYM_CHANNEL_TRY_SEND, SYM_EVENT_GET_HANDLER, SYM_EVENT_LEN, SYM_EVENT_SUBSCRIBE,
    SYM_EVENT_UNSUBSCRIBE_FIRST, SYM_FIBER_CANCEL, SYM_FIBER_CURRENT_ID, SYM_FIBER_DETACH,
    SYM_FIBER_JOIN, SYM_FIBER_JOIN_VALUE, SYM_FIBER_NOW_MILLIS, SYM_FIBER_SPAWN, SYM_FIBER_YIELD,
    SYM_GC_BYTES_ALLOCATED, SYM_GC_COLLECT, SYM_GC_COLLECT_IF_NEEDED, SYM_GC_EXTERNAL_ROOT_COUNT,
    SYM_GC_OBJECT_COUNT, SYM_GC_PHASE, SYM_GC_REGISTER_ROOT, SYM_GC_ROOT_HANDLE,
    SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER, SYM_HUB_CREATE,
    SYM_HUB_REGISTER, SYM_HUB_UNREGISTER, SYM_HUB_WAIT_RECEIVE, SYM_HUB_WAIT_RECEIVE_INDEX,
    SYM_HUB_WAIT_RECEIVE_VALUE, SYM_INTEROP_DISPATCH_PTR, SYM_INTEROP_DISPATCH_UNIT,
    SYM_INTEROP_DISPATCH_USIZE, SYM_MUTEX_CREATE, SYM_MUTEX_LOCK, SYM_MUTEX_TRY_LOCK,
    SYM_MUTEX_UNLOCK, SYM_PANIC, SYM_PANIC_STR, SYM_STR_CONCAT, SYM_STR_LEN, SYM_STR_NEW,
    SYM_SYSCALL_READ, SYM_SYSCALL_WRITE, SYM_TEST_BYTES_LEN, SYM_TEST_BYTES_PTR,
    SYM_WAIT_GROUP_ADD, SYM_WAIT_GROUP_CREATE, SYM_WAIT_GROUP_DONE, SYM_WAIT_GROUP_WAIT,
};
use beskid_codegen::cranelift_host::{
    ExternDeclarationError, HostError, declare_builtin_imports, declare_user_functions,
    declare_validated_extern_imports, remap_testcase_externals,
};
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use beskid_pipeline::{
    PipelineObserver, emit_work_unit, observe_phase_result,
    phases::{JIT_EMIT, JIT_FINALIZE},
};
use beskid_runtime::{
    alloc, array_len, array_new, channel_close, channel_create, channel_receive_status,
    channel_receive_value, channel_send, channel_try_receive, channel_try_send, event_get_handler,
    event_len, event_subscribe, event_unsubscribe_first, fiber_cancel, fiber_current_id,
    fiber_detach, fiber_join_status, fiber_join_value, fiber_now_millis, fiber_spawn, fiber_yield,
    gc_bytes_allocated, gc_collect, gc_collect_if_needed, gc_external_root_count, gc_object_count,
    gc_phase, gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle,
    gc_write_barrier, hub_create, hub_register, hub_unregister, hub_wait_receive_index,
    hub_wait_receive_status, hub_wait_receive_value, interop_dispatch_ptr, interop_dispatch_unit,
    interop_dispatch_usize, mutex_create, mutex_lock, mutex_try_lock, mutex_unlock, panic,
    panic_str, str_concat, str_len, str_new, syscall_read, syscall_write, test_bytes_len,
    test_bytes_ptr, wait_group_add, wait_group_create, wait_group_done, wait_group_wait,
};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, ModuleError, default_libcall_names};
use std::fmt;

/// Failure to build the ISA, declare/define Cranelift module objects, or resolve a symbol name.
#[derive(Debug)]
pub enum JitError {
    Isa(String),
    Module(Box<ModuleError>),
    MissingFunction(String),
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitError::Isa(msg) => write!(f, "{msg}"),
            JitError::Module(err) => write!(f, "{err}"),
            JitError::MissingFunction(name) => write!(f, "missing function `{name}`"),
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
            HostError::InvalidGlobalValue => {
                JitError::Isa("invalid global value for extern data symbol".to_owned())
            }
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
}

impl BeskidJitModule {
    /// JIT module with only built-in runtime symbols (no extra `dlopen` addresses).
    pub fn new() -> Result<Self, JitError> {
        let builder = new_builder(&[])?;

        let module = JITModule::new(builder);
        Ok(Self {
            module,
            func_ids: HashMap::new(),
            builtins_declared: false,
        })
    }

    /// JIT module that also registers `extras` as raw symbol addresses (for resolved extern imports).
    pub fn new_with_symbols(extras: &[(String, *const u8)]) -> Result<Self, JitError> {
        let builder = new_builder(extras)?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            func_ids: HashMap::new(),
            builtins_declared: false,
        })
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
        if !self.builtins_declared {
            declare_builtin_imports(&mut self.module, &mut self.func_ids)?;
            self.builtins_declared = true;
        }

        declare_user_functions(
            &mut self.module,
            artifact,
            Linkage::Local,
            &mut self.func_ids,
        )?;
        declare_validated_extern_imports(&mut self.module, artifact, &mut self.func_ids)?;

        emit_string_literals(&mut self.module, artifact)?;
        emit_type_descriptors(&mut self.module, artifact)?;

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
            emit_work_unit(
                pipeline,
                JIT_EMIT,
                (index as u64) + 1,
                total,
                function.name.clone(),
            );
        }

        observe_phase_result(pipeline, JIT_FINALIZE, || {
            self.module.finalize_definitions().map_err(JitError::from)
        })?;
        Ok(())
    }

    /// [`FuncId`] for a declared function or import symbol name, if present.
    pub fn get_func_id(&self, name: &str) -> Option<FuncId> {
        self.func_ids.get(name).copied()
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

fn new_builder(extras: &[(String, *const u8)]) -> Result<JITBuilder, JitError> {
    let isa_builder = cranelift_native::builder().map_err(|err| JitError::Isa(err.to_string()))?;
    let isa = isa_builder
        .finish(settings::Flags::new(settings::builder()))
        .map_err(|err| JitError::Isa(err.to_string()))?;
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    register_runtime_symbols(&mut builder);
    for (sym, addr) in extras {
        builder.symbol(sym, *addr);
    }
    Ok(builder)
}

fn register_runtime_symbols(builder: &mut JITBuilder) {
    builder.symbol(SYM_ALLOC, alloc as *const u8);
    builder.symbol(SYM_STR_NEW, str_new as *const u8);
    builder.symbol(SYM_STR_CONCAT, str_concat as *const u8);
    builder.symbol(SYM_ARRAY_NEW, array_new as *const u8);
    builder.symbol(SYM_ARRAY_LEN, array_len as *const u8);
    builder.symbol(SYM_PANIC, panic as *const u8);
    builder.symbol(SYM_PANIC_STR, panic_str as *const u8);
    builder.symbol(SYM_SYSCALL_WRITE, syscall_write as *const u8);
    builder.symbol(SYM_SYSCALL_READ, syscall_read as *const u8);
    builder.symbol(SYM_STR_LEN, str_len as *const u8);
    builder.symbol(
        SYM_INTEROP_DISPATCH_UNIT,
        interop_dispatch_unit as *const u8,
    );
    builder.symbol(SYM_INTEROP_DISPATCH_PTR, interop_dispatch_ptr as *const u8);
    builder.symbol(
        SYM_INTEROP_DISPATCH_USIZE,
        interop_dispatch_usize as *const u8,
    );
    builder.symbol(SYM_GC_WRITE_BARRIER, gc_write_barrier as *const u8);
    builder.symbol(SYM_GC_BYTES_ALLOCATED, gc_bytes_allocated as *const u8);
    builder.symbol(SYM_GC_OBJECT_COUNT, gc_object_count as *const u8);
    builder.symbol(SYM_GC_PHASE, gc_phase as *const u8);
    builder.symbol(SYM_GC_COLLECT, gc_collect as *const u8);
    builder.symbol(SYM_GC_COLLECT_IF_NEEDED, gc_collect_if_needed as *const u8);
    builder.symbol(
        SYM_GC_EXTERNAL_ROOT_COUNT,
        gc_external_root_count as *const u8,
    );
    builder.symbol(SYM_GC_ROOT_HANDLE, gc_root_handle as *const u8);
    builder.symbol(SYM_GC_UNROOT_HANDLE, gc_unroot_handle as *const u8);
    builder.symbol(SYM_GC_REGISTER_ROOT, gc_register_root as *const u8);
    builder.symbol(SYM_GC_UNREGISTER_ROOT, gc_unregister_root as *const u8);
    builder.symbol(SYM_EVENT_SUBSCRIBE, event_subscribe as *const u8);
    builder.symbol(
        SYM_EVENT_UNSUBSCRIBE_FIRST,
        event_unsubscribe_first as *const u8,
    );
    builder.symbol(SYM_EVENT_LEN, event_len as *const u8);
    builder.symbol(SYM_EVENT_GET_HANDLER, event_get_handler as *const u8);
    builder.symbol(SYM_TEST_BYTES_PTR, test_bytes_ptr as *const u8);
    builder.symbol(SYM_TEST_BYTES_LEN, test_bytes_len as *const u8);
    builder.symbol(SYM_FIBER_SPAWN, fiber_spawn as *const u8);
    builder.symbol(SYM_FIBER_JOIN, fiber_join_status as *const u8);
    builder.symbol(SYM_FIBER_JOIN_VALUE, fiber_join_value as *const u8);
    builder.symbol(SYM_FIBER_DETACH, fiber_detach as *const u8);
    builder.symbol(SYM_FIBER_CANCEL, fiber_cancel as *const u8);
    builder.symbol(SYM_FIBER_YIELD, fiber_yield as *const u8);
    builder.symbol(SYM_FIBER_NOW_MILLIS, fiber_now_millis as *const u8);
    builder.symbol(SYM_FIBER_CURRENT_ID, fiber_current_id as *const u8);
    builder.symbol(SYM_CHANNEL_CREATE, channel_create as *const u8);
    builder.symbol(SYM_CHANNEL_SEND, channel_send as *const u8);
    builder.symbol(SYM_CHANNEL_RECEIVE, channel_receive_status as *const u8);
    builder.symbol(
        SYM_CHANNEL_RECEIVE_VALUE,
        channel_receive_value as *const u8,
    );
    builder.symbol(SYM_CHANNEL_TRY_SEND, channel_try_send as *const u8);
    builder.symbol(SYM_CHANNEL_TRY_RECEIVE, channel_try_receive as *const u8);
    builder.symbol(SYM_CHANNEL_CLOSE, channel_close as *const u8);
    builder.symbol(SYM_HUB_CREATE, hub_create as *const u8);
    builder.symbol(SYM_HUB_REGISTER, hub_register as *const u8);
    builder.symbol(SYM_HUB_UNREGISTER, hub_unregister as *const u8);
    builder.symbol(SYM_HUB_WAIT_RECEIVE, hub_wait_receive_status as *const u8);
    builder.symbol(
        SYM_HUB_WAIT_RECEIVE_INDEX,
        hub_wait_receive_index as *const u8,
    );
    builder.symbol(
        SYM_HUB_WAIT_RECEIVE_VALUE,
        hub_wait_receive_value as *const u8,
    );
    builder.symbol(SYM_MUTEX_CREATE, mutex_create as *const u8);
    builder.symbol(SYM_MUTEX_LOCK, mutex_lock as *const u8);
    builder.symbol(SYM_MUTEX_TRY_LOCK, mutex_try_lock as *const u8);
    builder.symbol(SYM_MUTEX_UNLOCK, mutex_unlock as *const u8);
    builder.symbol(SYM_WAIT_GROUP_CREATE, wait_group_create as *const u8);
    builder.symbol(SYM_WAIT_GROUP_ADD, wait_group_add as *const u8);
    builder.symbol(SYM_WAIT_GROUP_DONE, wait_group_done as *const u8);
    builder.symbol(SYM_WAIT_GROUP_WAIT, wait_group_wait as *const u8);
}
