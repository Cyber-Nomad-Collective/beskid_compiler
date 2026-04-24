use std::collections::HashMap;

use beskid_abi::{
    AbiParamKind, AbiReturnKind, BUILTIN_SPECS, SYM_ALLOC, SYM_ARRAY_NEW, SYM_EVENT_GET_HANDLER,
    SYM_EVENT_LEN, SYM_EVENT_SUBSCRIBE, SYM_EVENT_UNSUBSCRIBE_FIRST, SYM_GC_REGISTER_ROOT,
    SYM_GC_ROOT_HANDLE, SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER,
    SYM_INTEROP_DISPATCH_PTR, SYM_INTEROP_DISPATCH_UNIT, SYM_INTEROP_DISPATCH_USIZE, SYM_PANIC,
    SYM_PANIC_STR, SYM_STR_CONCAT, SYM_STR_LEN, SYM_STR_NEW, SYM_SYSCALL_WRITE,
    SYM_TEST_BYTES_LEN, SYM_TEST_BYTES_PTR,
};
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use beskid_runtime::{
    alloc, array_new, event_get_handler, event_len, event_subscribe, event_unsubscribe_first,
    gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier,
    interop_dispatch_ptr, interop_dispatch_unit, interop_dispatch_usize, panic, panic_str,
    str_concat, str_len, str_new, syscall_write, test_bytes_len, test_bytes_ptr,
};
use cranelift_codegen::ir::{AbiParam, ExternalName, Signature, UserExternalName, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, FuncOrDataId, Linkage, Module, ModuleError, default_libcall_names};

#[derive(Debug)]
pub enum JitError {
    Isa(String),
    Module(ModuleError),
    MissingFunction(String),
}

impl From<ModuleError> for JitError {
    fn from(error: ModuleError) -> Self {
        Self::Module(error)
    }
}

pub struct BeskidJitModule {
    module: JITModule,
    func_ids: HashMap<String, FuncId>,
    builtins_declared: bool,
}

impl BeskidJitModule {
    pub fn new() -> Result<Self, JitError> {
        let isa_builder =
            cranelift_native::builder().map_err(|err| JitError::Isa(err.to_string()))?;
        let isa = isa_builder
            .finish(settings::Flags::new(settings::builder()))
            .map_err(|err| JitError::Isa(err.to_string()))?;
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        builder.symbol(SYM_ALLOC, alloc as *const u8);
        builder.symbol(SYM_STR_NEW, str_new as *const u8);
        builder.symbol(SYM_STR_CONCAT, str_concat as *const u8);
        builder.symbol(SYM_ARRAY_NEW, array_new as *const u8);
        builder.symbol(SYM_PANIC, panic as *const u8);
        builder.symbol(SYM_PANIC_STR, panic_str as *const u8);
        builder.symbol(SYM_SYSCALL_WRITE, syscall_write as *const u8);
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

        let module = JITModule::new(builder);
        Ok(Self {
            module,
            func_ids: HashMap::new(),
            builtins_declared: false,
        })
    }

    pub fn new_with_symbols(extras: &[(String, *const u8)]) -> Result<Self, JitError> {
        let isa_builder =
            cranelift_native::builder().map_err(|err| JitError::Isa(err.to_string()))?;
        let isa = isa_builder
            .finish(settings::Flags::new(settings::builder()))
            .map_err(|err| JitError::Isa(err.to_string()))?;
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        // Register runtime builtins
        builder.symbol(SYM_ALLOC, alloc as *const u8);
        builder.symbol(SYM_STR_NEW, str_new as *const u8);
        builder.symbol(SYM_STR_CONCAT, str_concat as *const u8);
        builder.symbol(SYM_ARRAY_NEW, array_new as *const u8);
        builder.symbol(SYM_PANIC, panic as *const u8);
        builder.symbol(SYM_PANIC_STR, panic_str as *const u8);
        builder.symbol(SYM_SYSCALL_WRITE, syscall_write as *const u8);
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

        for (sym, addr) in extras {
            builder.symbol(sym, *addr);
        }

        let module = JITModule::new(builder);
        Ok(Self {
            module,
            func_ids: HashMap::new(),
            builtins_declared: false,
        })
    }

    pub fn compile(&mut self, artifact: &CodegenArtifact) -> Result<(), JitError> {
        if !self.builtins_declared {
            self.declare_builtins()?;
            self.builtins_declared = true;
        }

        // First pass: declare user functions
        for function in &artifact.functions {
            let func_id = self.module.declare_function(
                &function.name,
                Linkage::Local,
                &function.function.signature,
            )?;
            self.func_ids.insert(function.name.clone(), func_id);
        }

        // Second pass: collect extern references and declare them with their IR signatures
        let pointer = self.module.isa().pointer_type();
        let mut extern_sigs: std::collections::HashMap<String, Signature> = Default::default();
        {
            let mut ctx_probe = self.module.make_context();
            for function in &artifact.functions {
                ctx_probe.func = function.function.clone();
                for (_func_ref, ext_func) in ctx_probe.func.dfg.ext_funcs.iter() {
                    if let ExternalName::TestCase(name) = &ext_func.name {
                        let symbol = String::from_utf8_lossy(name.raw()).to_string();
                        // Only consider externs surfaced in artifact.extern_imports (avoid builtins remap here)
                        if artifact.extern_imports.iter().any(|e| e.symbol == symbol) {
                            let sig = ctx_probe.func.dfg.signatures[ext_func.signature].clone();
                            // Validate FFI signature is allowed
                            validate_ffi_signature(&sig, pointer).map_err(|msg| {
                                JitError::Isa(format!(
                                    "extern signature not allowed for {}: {}",
                                    symbol, msg
                                ))
                            })?;
                            if let Some(prev) = extern_sigs.get(&symbol) {
                                if prev != &sig {
                                    return Err(JitError::Isa(format!(
                                        "extern signature mismatch for {} across callsites",
                                        symbol
                                    )));
                                }
                            } else {
                                extern_sigs.insert(symbol, sig);
                            }
                        }
                    }
                }
                self.module.clear_context(&mut ctx_probe);
            }
        }

        for (symbol, sig) in extern_sigs.iter() {
            let id = self.module.declare_function(symbol, Linkage::Import, sig)?;
            self.func_ids.insert(symbol.clone(), id);
        }

        emit_string_literals(&mut self.module, artifact)?;
        emit_type_descriptors(&mut self.module, artifact)?;

        let mut ctx = self.module.make_context();
        for function in &artifact.functions {
            let func_id = self
                .func_ids
                .get(&function.name)
                .copied()
                .ok_or_else(|| JitError::MissingFunction(function.name.clone()))?;
            ctx.func = function.function.clone();
            remap_external_testcase_names(&mut ctx, &self.module, &self.func_ids)?;
            self.module.define_function(func_id, &mut ctx)?;
            self.module.clear_context(&mut ctx);
        }

        self.module.finalize_definitions()?;
        Ok(())
    }

    pub fn get_func_id(&self, name: &str) -> Option<FuncId> {
        self.func_ids.get(name).copied()
    }

    pub unsafe fn get_finalized_function_ptr(&mut self, func_id: FuncId) -> *const u8 {
        self.module.get_finalized_function(func_id)
    }

    pub fn module(&mut self) -> &mut JITModule {
        &mut self.module
    }

    fn declare_builtins(&mut self) -> Result<(), JitError> {
        let pointer = self.module.isa().pointer_type();

        let call_conv = self.module.isa().default_call_conv();
        for spec in BUILTIN_SPECS {
            let signature = builtin_signature(pointer, call_conv, spec.params, spec.returns);
            let id = self
                .module
                .declare_function(spec.symbol, Linkage::Import, &signature)?;
            self.func_ids.insert(spec.symbol.to_owned(), id);
        }

        let mut declare = |symbol: &str, params: &[AbiParamKind], returns: AbiReturnKind| {
            let signature = builtin_signature(pointer, call_conv, params, returns);
            let id = self
                .module
                .declare_function(symbol, Linkage::Import, &signature)?;
            self.func_ids.insert(symbol.to_owned(), id);
            Ok::<(), ModuleError>(())
        };
        declare(
            SYM_EVENT_SUBSCRIBE,
            &[AbiParamKind::Ptr, AbiParamKind::Ptr, AbiParamKind::Ptr],
            AbiReturnKind::I64,
        )?;
        declare(
            SYM_EVENT_UNSUBSCRIBE_FIRST,
            &[AbiParamKind::Ptr, AbiParamKind::Ptr],
            AbiReturnKind::I64,
        )?;
        declare(SYM_EVENT_LEN, &[AbiParamKind::Ptr], AbiReturnKind::I64)?;
        declare(
            SYM_EVENT_GET_HANDLER,
            &[AbiParamKind::Ptr, AbiParamKind::Ptr],
            AbiReturnKind::Ptr,
        )?;

        Ok(())
    }
}

fn validate_ffi_signature(
    sig: &Signature,
    pointer: cranelift_codegen::ir::Type,
) -> Result<(), String> {
    use cranelift_codegen::ir::types;
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

fn builtin_signature(
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

fn remap_external_testcase_names(
    ctx: &mut cranelift_codegen::Context,
    module: &JITModule,
    func_ids: &HashMap<String, FuncId>,
) -> Result<(), JitError> {
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
            .ok_or_else(|| JitError::MissingFunction(symbol.clone()))?;
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
            .ok_or_else(|| JitError::MissingFunction(symbol.clone()))?;
        let FuncOrDataId::Data(data_id) = id else {
            return Err(JitError::MissingFunction(symbol));
        };
        let user_ref = ctx.func.declare_imported_user_function(UserExternalName {
            namespace: 1,
            index: data_id.as_u32(),
        });
        let cranelift_codegen::ir::GlobalValueData::Symbol { name, .. } =
            &mut ctx.func.global_values[gv]
        else {
            return Err(JitError::MissingFunction(symbol));
        };
        *name = ExternalName::user(user_ref);
    }
    Ok(())
}
