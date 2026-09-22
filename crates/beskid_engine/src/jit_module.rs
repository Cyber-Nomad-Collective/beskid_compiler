use std::collections::{HashMap, HashSet};

use crate::runtime_kit::JitRuntimeKit;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_codegen::cranelift_host::{
    ExternDeclarationError, HostError, declare_user_functions, declare_validated_extern_imports,
    remap_testcase_externals,
};
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use beskid_pipeline::{
    PipelineObserver, emit_work_unit, observe_phase_result,
    phases::{JIT_EMIT, JIT_FINALIZE},
};
use cranelift_codegen::{
    ir::ExternalName,
    isa::{self, TargetIsa},
    settings::{self, Configurable},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, ModuleError, default_libcall_names};
use std::fmt;
use std::sync::Arc;
use target_lexicon::Architecture;

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

fn validate_exact_symbol_references(
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

fn new_builder(extras: &[(String, *const u8)]) -> Result<JITBuilder, JitError> {
    let isa = native_jit_isa()?;
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    for (sym, addr) in extras {
        builder.symbol(sym, *addr);
    }
    Ok(builder)
}

/// Construct the native ISA shared by JIT lowering and final emission.
///
/// This is the sole owner of JIT-only relocation policy. The shared codegen settings retain
/// frame pointers for the tail-call invariant; JIT then selects x86_64 PIC from the target ISA
/// and disables colocated libcalls for every native target.
pub(crate) fn native_jit_isa() -> Result<Arc<dyn TargetIsa>, JitError> {
    let builder = cranelift_native::builder().map_err(|error| JitError::Isa(error.to_string()))?;
    native_jit_isa_from_builder(builder)
}

fn native_jit_isa_from_builder(builder: isa::Builder) -> Result<Arc<dyn TargetIsa>, JitError> {
    let is_x86_64 = matches!(builder.triple().architecture, Architecture::X86_64);
    let mut settings = beskid_codegen::cranelift_host::production_isa_settings_builder()
        .map_err(|error| JitError::Isa(error.to_string()))?;
    settings
        .set("use_colocated_libcalls", "false")
        .map_err(|error| JitError::Isa(format!("native JIT libcall relocation policy failed: {error}")))?;
    settings
        .set("is_pic", if is_x86_64 { "true" } else { "false" })
        .map_err(|error| JitError::Isa(format!("native JIT PIC policy failed: {error}")))?;
    builder.finish(settings::Flags::new(settings)).map_err(|error| JitError::Isa(error.to_string()))
}

#[cfg(all(test, target_arch = "x86_64", any(unix, windows)))]
mod native_jit_policy_tests {
    use super::*;
    use cranelift_codegen::{
        binemit::Reloc,
        ir::{AbiParam, InstBuilder, UserFuncName, types},
    };
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{BranchProtection, JITMemoryKind, JITMemoryProvider};
    use cranelift_module::{DataDescription, ModuleReloc, ModuleRelocTarget, ModuleResult};
    use std::{
        collections::VecDeque,
        io, mem, ptr,
        sync::{Arc, Mutex},
    };

    const FAR_SPAN: usize = (i32::MAX as usize) + (16 * 1024 * 1024);
    const VENEER_SIZE: usize = 16;

    #[derive(Clone, Copy)]
    enum Placement {
        Low,
        High,
    }

    #[derive(Clone, Copy)]
    enum FinalProtection {
        ReadExecute,
        ReadOnly,
        ReadWrite,
    }

    struct Allocation {
        address: usize,
        len: usize,
        final_protection: FinalProtection,
    }

    /// Test-only virtual address reservation. Allocation failure is returned to Cranelift instead
    /// of turning a far-address regression into a skipped test.
    struct ReservedAddressSpace {
        base: usize,
        len: usize,
        page_size: usize,
    }

    impl ReservedAddressSpace {
        fn new() -> io::Result<Self> {
            let page_size = platform::page_size()?;
            let len = FAR_SPAN
                .checked_add(page_size)
                .ok_or_else(|| io::Error::other("far-address reservation length overflow"))?
                .next_multiple_of(page_size);
            let base = platform::reserve(len)?;
            Ok(Self { base, len, page_size })
        }
    }

    impl Drop for ReservedAddressSpace {
        fn drop(&mut self) {
            // A failed release is not recoverable for this test support. Avoid panicking during
            // stack unwinding; the allocation operations report their own diagnostics.
            let _ = unsafe { platform::release(self.base, self.len) };
        }
    }

    /// Places JIT allocations at opposite ends of one reservation. This makes every asserted
    /// far relocation larger than a signed 32-bit displacement without committing the gap.
    struct FarMemoryProvider {
        space: ReservedAddressSpace,
        executable_placements: VecDeque<Placement>,
        readonly_placements: VecDeque<Placement>,
        writable_placements: VecDeque<Placement>,
        low_offset: usize,
        high_offset: usize,
        allocations: Vec<Allocation>,
        requested_exec_sizes: Arc<Mutex<Vec<usize>>>,
    }

    impl FarMemoryProvider {
        fn new(
            executable_placements: impl IntoIterator<Item = Placement>,
            readonly_placements: impl IntoIterator<Item = Placement>,
        ) -> io::Result<(Self, Arc<Mutex<Vec<usize>>>)> {
            let space = ReservedAddressSpace::new()?;
            let high_offset = space.len;
            let requested_exec_sizes = Arc::new(Mutex::new(Vec::new()));
            Ok((
                Self {
                    space,
                    executable_placements: executable_placements.into_iter().collect(),
                    readonly_placements: readonly_placements.into_iter().collect(),
                    writable_placements: VecDeque::new(),
                    low_offset: 0,
                    high_offset,
                    allocations: Vec::new(),
                    requested_exec_sizes: Arc::clone(&requested_exec_sizes),
                },
                requested_exec_sizes,
            ))
        }

        fn allocate_at(
            &mut self,
            size: usize,
            align: u64,
            placement: Placement,
            final_protection: FinalProtection,
        ) -> io::Result<*mut u8> {
            let align = usize::try_from(align).map_err(|_| io::Error::other("JIT alignment exceeds usize"))?;
            if align > self.space.page_size {
                return Err(io::Error::other(format!(
                    "JIT alignment {align} exceeds reserved-address page size {}",
                    self.space.page_size
                )));
            }
            let len = size.next_multiple_of(self.space.page_size).max(self.space.page_size);
            let address = match placement {
                Placement::Low => {
                    let address = self
                        .space
                        .base
                        .checked_add(self.low_offset)
                        .ok_or_else(|| io::Error::other("low far allocation overflow"))?;
                    self.low_offset = self
                        .low_offset
                        .checked_add(len)
                        .ok_or_else(|| io::Error::other("low far allocation length overflow"))?;
                    address
                }
                Placement::High => {
                    self.high_offset = self
                        .high_offset
                        .checked_sub(len)
                        .ok_or_else(|| io::Error::other("high far allocation exhausted reservation"))?;
                    self.space
                        .base
                        .checked_add(self.high_offset)
                        .ok_or_else(|| io::Error::other("high far allocation overflow"))?
                }
            };
            if self.low_offset > self.high_offset {
                return Err(io::Error::other("far JIT allocations overlap reserved address space"));
            }
            unsafe { platform::make_writable(address, len)? };
            self.allocations.push(Allocation { address, len, final_protection });
            Ok(address as *mut u8)
        }
    }

    unsafe impl Send for FarMemoryProvider {}

    impl JITMemoryProvider for FarMemoryProvider {
        fn allocate(&mut self, size: usize, align: u64, kind: JITMemoryKind) -> io::Result<*mut u8> {
            let (placement, final_protection) = match kind {
                JITMemoryKind::Executable => {
                    self.requested_exec_sizes.lock().expect("record executable allocation").push(size);
                    (
                        self.executable_placements
                            .pop_front()
                            .ok_or_else(|| io::Error::other("missing required executable far placement"))?,
                        FinalProtection::ReadExecute,
                    )
                }
                JITMemoryKind::Writable => (
                    self.writable_placements
                        .pop_front()
                        .ok_or_else(|| io::Error::other("missing required writable far placement"))?,
                    FinalProtection::ReadWrite,
                ),
                JITMemoryKind::ReadOnly => (
                    self.readonly_placements
                        .pop_front()
                        .ok_or_else(|| io::Error::other("missing required read-only far placement"))?,
                    FinalProtection::ReadOnly,
                ),
            };
            self.allocate_at(size, align, placement, final_protection)
        }

        unsafe fn free_memory(&mut self) {
            // The reservation owns every committed page and drops after JIT module disposal.
            self.allocations.clear();
        }

        fn finalize(&mut self, _branch_protection: BranchProtection) -> ModuleResult<()> {
            for allocation in &self.allocations {
                unsafe {
                    platform::protect(allocation.address, allocation.len, allocation.final_protection)
                        .map_err(|error| ModuleError::Backend(anyhow::Error::from(error)))?;
                }
            }
            Ok(())
        }
    }

    fn far_builder(
        executable_placements: impl IntoIterator<Item = Placement>,
        readonly_placements: impl IntoIterator<Item = Placement>,
    ) -> (JITBuilder, Arc<Mutex<Vec<usize>>>) {
        let (memory, executable_sizes) = FarMemoryProvider::new(executable_placements, readonly_placements)
            .expect("reserve more than signed-i32 displacement for required native JIT regression");
        let mut builder = new_builder(&[]).expect("construct the production native JIT builder");
        builder.memory_provider(Box::new(memory));
        (builder, executable_sizes)
    }

    fn branch_destination(caller: *const u8, reloc_offset: usize) -> *const u8 {
        let at = caller.wrapping_byte_add(reloc_offset);
        let displacement = unsafe { at.cast::<i32>().read_unaligned() };
        at.wrapping_byte_add(4).wrapping_byte_offset(displacement as isize)
    }

    fn assert_veneer(caller: *const u8, code_len: usize, reloc_offset: usize, expected_target: *const u8) {
        assert!(
            caller.addr().abs_diff(expected_target.addr()) > i32::MAX as usize,
            "forced-far provider did not establish a displacement beyond signed i32"
        );
        let veneer = branch_destination(caller, reloc_offset);
        assert_eq!(veneer, caller.wrapping_byte_add(code_len));
        assert_eq!(unsafe { std::slice::from_raw_parts(veneer, 6) }, [0xff, 0x25, 0, 0, 0, 0]);
        assert_eq!(unsafe { veneer.byte_add(6).cast::<u64>().read_unaligned() }, expected_target.addr() as u64);
    }

    fn assert_near_direct_call(caller: *const u8, reloc_offset: usize, expected_target: *const u8) {
        let branch = caller.wrapping_byte_add(reloc_offset);
        assert!(
            branch.addr().abs_diff(expected_target.addr()) <= i32::MAX as usize,
            "mixed provider did not establish an in-range direct-call displacement"
        );
        assert_eq!(branch_destination(caller, reloc_offset), expected_target);
    }

    #[test]
    fn far_x86_production_builder_executes_direct_and_tail_transfers_in_both_directions() {
        let (builder, requested_exec_sizes) =
            far_builder([Placement::Low, Placement::High, Placement::Low, Placement::High], []);
        let mut module = JITModule::new(builder);
        let mut signature = module.make_signature();
        signature.returns.push(AbiParam::new(types::I32));

        let low_callee = module.declare_function("low_callee", Linkage::Local, &signature).unwrap();
        let high_caller = module.declare_function("high_caller", Linkage::Local, &signature).unwrap();
        let low_tail_caller = module.declare_function("low_tail_caller", Linkage::Local, &signature).unwrap();
        let high_tail_callee = module.declare_function("high_tail_callee", Linkage::Local, &signature).unwrap();

        module.define_function_bytes(low_callee, 1, &[0xb8, 42, 0, 0, 0, 0xc3], &[]).unwrap();
        let mut context = module.make_context();
        context.func.name = UserFuncName::user(0, high_caller.as_u32());
        context.func.signature = signature.clone();
        let low_callee_ref = module.declare_func_in_func(low_callee, &mut context.func);
        let mut function_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut context.func, &mut function_context);
            let block = function.create_block();
            function.switch_to_block(block);
            let call = function.ins().call(low_callee_ref, &[]);
            let result = function.inst_results(call)[0];
            function.ins().return_(&[result]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(high_caller, &mut context).unwrap();
        let (high_caller_code_len, high_caller_reloc_offset) = {
            let compiled = context.compiled_code().expect("compiled direct call");
            let relocation = compiled
                .buffer
                .relocs()
                .iter()
                .find(|relocation| relocation.kind == Reloc::X86CallPCRel4)
                .expect("direct call relocation");
            (compiled.code_buffer().len(), relocation.offset as usize)
        };

        let tail_relocations = [ModuleReloc {
            offset: 1,
            kind: Reloc::X86CallPCRel4,
            name: ModuleRelocTarget::user(0, high_tail_callee.as_u32()),
            addend: -4,
        }];
        module.define_function_bytes(low_tail_caller, 1, &[0xe9, 0, 0, 0, 0], &tail_relocations).unwrap();
        module.define_function_bytes(high_tail_callee, 1, &[0xb8, 84, 0, 0, 0, 0xc3], &[]).unwrap();
        module
            .finalize_definitions()
            .expect("production PIC JIT policy must finalize both far control-transfer directions");

        let low_callee_ptr = module.get_finalized_function(low_callee);
        let high_caller_ptr = module.get_finalized_function(high_caller);
        let low_tail_caller_ptr = module.get_finalized_function(low_tail_caller);
        let high_tail_callee_ptr = module.get_finalized_function(high_tail_callee);
        assert!(high_caller_ptr.addr() > low_callee_ptr.addr());
        assert_veneer(high_caller_ptr, high_caller_code_len, high_caller_reloc_offset, low_callee_ptr);
        assert!(low_tail_caller_ptr.addr() < high_tail_callee_ptr.addr());
        assert_veneer(low_tail_caller_ptr, 5, 1, high_tail_callee_ptr);
        let direct: extern "C" fn() -> u32 = unsafe { mem::transmute(high_caller_ptr) };
        let tail: extern "C" fn() -> u32 = unsafe { mem::transmute(low_tail_caller_ptr) };
        assert_eq!(direct(), 42);
        assert_eq!(tail(), 84);
        let sizes = requested_exec_sizes.lock().expect("read executable allocation sizes");
        assert_eq!(sizes[1], high_caller_code_len + VENEER_SIZE);
        assert_eq!(sizes[2], 5 + VENEER_SIZE);
        drop(sizes);
        unsafe { module.free_memory() };
    }

    #[test]
    fn x86_production_builder_executes_near_direct_calls_with_far_function_and_data_addresses() {
        let (builder, _) =
            far_builder([Placement::Low, Placement::Low, Placement::Low, Placement::High], [Placement::High]);
        let mut module = JITModule::new(builder);
        let data_id = module.declare_anonymous_data(false, false).unwrap();
        let mut data = DataDescription::new();
        data.define(vec![42].into_boxed_slice());
        module.define_data(data_id, &data).unwrap();

        let mut call_signature = module.make_signature();
        call_signature.returns.push(AbiParam::new(types::I32));
        let near_target = module.declare_function("mixed_near_target", Linkage::Local, &call_signature).unwrap();
        let near_caller = module.declare_function("mixed_near_caller", Linkage::Local, &call_signature).unwrap();
        module.define_function_bytes(near_target, 1, &[0xb8, 42, 0, 0, 0, 0xc3], &[]).unwrap();

        let mut caller_context = module.make_context();
        caller_context.func.name = UserFuncName::user(0, near_caller.as_u32());
        caller_context.func.signature = call_signature;
        let near_target_reference = module.declare_func_in_func(near_target, &mut caller_context.func);
        let mut caller_builder_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut caller_context.func, &mut caller_builder_context);
            let block = function.create_block();
            function.switch_to_block(block);
            let call = function.ins().call(near_target_reference, &[]);
            let result = function.inst_results(call)[0];
            function.ins().return_(&[result]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(near_caller, &mut caller_context).unwrap();
        let near_caller_reloc_offset = caller_context
            .compiled_code()
            .expect("compiled near direct call")
            .buffer
            .relocs()
            .iter()
            .find(|relocation| relocation.kind == Reloc::X86CallPCRel4)
            .expect("near direct-call relocation")
            .offset as usize;

        let mut address_signature = module.make_signature();
        address_signature.returns.push(AbiParam::new(module.target_config().pointer_type()));
        let data_address = module.declare_function("mixed_data_address", Linkage::Local, &address_signature).unwrap();
        let function_address =
            module.declare_function("mixed_function_address", Linkage::Local, &address_signature).unwrap();

        let mut data_context = module.make_context();
        data_context.func.name = UserFuncName::user(0, data_address.as_u32());
        data_context.func.signature = address_signature.clone();
        let data_reference = module.declare_data_in_func(data_id, &mut data_context.func);
        let mut data_builder_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut data_context.func, &mut data_builder_context);
            let block = function.create_block();
            function.switch_to_block(block);
            let address = function.ins().symbol_value(module.target_config().pointer_type(), data_reference);
            function.ins().return_(&[address]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(data_address, &mut data_context).unwrap();

        let mut function_context = module.make_context();
        function_context.func.name = UserFuncName::user(0, function_address.as_u32());
        function_context.func.signature = address_signature;
        let target_reference = module.declare_func_in_func(near_target, &mut function_context.func);
        let mut function_builder_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut function_context.func, &mut function_builder_context);
            let block = function.create_block();
            function.switch_to_block(block);
            let address = function.ins().func_addr(module.target_config().pointer_type(), target_reference);
            function.ins().return_(&[address]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(function_address, &mut function_context).unwrap();
        module.finalize_definitions().expect("production JIT policy must support mixed near and far relocations");

        let target_pointer = module.get_finalized_function(near_target);
        let caller_pointer = module.get_finalized_function(near_caller);
        let data_address_pointer = module.get_finalized_function(data_address);
        let function_address_pointer = module.get_finalized_function(function_address);
        let (data_pointer, _) = module.get_finalized_data(data_id);
        assert_near_direct_call(caller_pointer, near_caller_reloc_offset, target_pointer);
        assert!(data_address_pointer.addr().abs_diff(data_pointer.addr()) > i32::MAX as usize);
        assert!(function_address_pointer.addr().abs_diff(target_pointer.addr()) > i32::MAX as usize);
        let call_near: extern "C" fn() -> u32 = unsafe { mem::transmute(caller_pointer) };
        let load_data_address: extern "C" fn() -> usize = unsafe { mem::transmute(data_address_pointer) };
        let load_function_address: extern "C" fn() -> usize = unsafe { mem::transmute(function_address_pointer) };
        assert_eq!(call_near(), 42);
        assert_eq!(load_data_address(), data_pointer.addr());
        assert_eq!(load_function_address(), target_pointer.addr());
        unsafe { module.free_memory() };
    }

    #[test]
    fn far_x86_production_builder_preserves_function_and_data_address_identity() {
        let (builder, _) = far_builder([Placement::High, Placement::Low, Placement::Low], [Placement::High]);
        let mut module = JITModule::new(builder);
        let data_id = module.declare_anonymous_data(false, false).unwrap();
        let mut data = DataDescription::new();
        data.define(vec![42].into_boxed_slice());
        module.define_data(data_id, &data).unwrap();

        let target_signature = module.make_signature();
        let target = module.declare_function("address_target", Linkage::Local, &target_signature).unwrap();
        module.define_function_bytes(target, 1, &[0xc3], &[]).unwrap();
        let mut address_signature = module.make_signature();
        address_signature.returns.push(AbiParam::new(module.target_config().pointer_type()));
        let data_address = module.declare_function("data_address", Linkage::Local, &address_signature).unwrap();
        let function_address = module.declare_function("function_address", Linkage::Local, &address_signature).unwrap();

        let mut data_context = module.make_context();
        data_context.func.name = UserFuncName::user(0, data_address.as_u32());
        data_context.func.signature = address_signature.clone();
        let data_reference = module.declare_data_in_func(data_id, &mut data_context.func);
        let mut function_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut data_context.func, &mut function_context);
            let block = function.create_block();
            function.switch_to_block(block);
            let address = function.ins().symbol_value(module.target_config().pointer_type(), data_reference);
            function.ins().return_(&[address]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(data_address, &mut data_context).unwrap();

        let mut function_context = module.make_context();
        function_context.func.name = UserFuncName::user(0, function_address.as_u32());
        function_context.func.signature = address_signature;
        let target_reference = module.declare_func_in_func(target, &mut function_context.func);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut function = FunctionBuilder::new(&mut function_context.func, &mut builder_context);
            let block = function.create_block();
            function.switch_to_block(block);
            function.ins().call(target_reference, &[]);
            let address = function.ins().func_addr(module.target_config().pointer_type(), target_reference);
            function.ins().return_(&[address]);
            function.seal_all_blocks();
            function.finalize(module.target_config());
        }
        module.define_function(function_address, &mut function_context).unwrap();

        // With the old duplicated custom-ISA construction this panics while applying an
        // out-of-range X86PCRel4 data/function-address relocation. It must turn into a GOT
        // relocation through the single production PIC owner.
        module.finalize_definitions().expect("production PIC JIT policy must resolve far function and data addresses");
        assert!(
            data_context
                .compiled_code()
                .expect("compiled data address")
                .buffer
                .relocs()
                .iter()
                .any(|relocation| relocation.kind == Reloc::X86GOTPCRel4)
        );
        assert!(
            function_context
                .compiled_code()
                .expect("compiled function address")
                .buffer
                .relocs()
                .iter()
                .any(|relocation| relocation.kind == Reloc::X86GOTPCRel4)
        );

        let (data_pointer, _) = module.get_finalized_data(data_id);
        let target_pointer = module.get_finalized_function(target);
        let data_address_pointer = module.get_finalized_function(data_address);
        let function_address_pointer = module.get_finalized_function(function_address);
        assert!(data_address_pointer.addr().abs_diff(data_pointer.addr()) > i32::MAX as usize);
        assert!(function_address_pointer.addr().abs_diff(target_pointer.addr()) > i32::MAX as usize);
        let load_data_address: extern "C" fn() -> usize = unsafe { mem::transmute(data_address_pointer) };
        let load_function_address: extern "C" fn() -> usize = unsafe { mem::transmute(function_address_pointer) };
        assert_eq!(load_data_address(), data_pointer.addr());
        assert_eq!(load_function_address(), target_pointer.addr());
        unsafe { module.free_memory() };
    }

    #[cfg(unix)]
    mod platform {
        use super::{FinalProtection, io, ptr};

        pub(super) fn page_size() -> io::Result<usize> {
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            usize::try_from(page_size).map_err(|_| io::Error::last_os_error())
        }

        pub(super) fn reserve(len: usize) -> io::Result<usize> {
            let mapping =
                unsafe { libc::mmap(ptr::null_mut(), len, libc::PROT_NONE, libc::MAP_PRIVATE | libc::MAP_ANON, -1, 0) };
            if mapping == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            Ok(mapping.addr())
        }

        pub(super) unsafe fn make_writable(address: usize, len: usize) -> io::Result<()> {
            if unsafe { libc::mprotect(address as *mut libc::c_void, len, libc::PROT_READ | libc::PROT_WRITE) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) unsafe fn protect(address: usize, len: usize, protection: FinalProtection) -> io::Result<()> {
            let protection = match protection {
                FinalProtection::ReadExecute => libc::PROT_READ | libc::PROT_EXEC,
                FinalProtection::ReadOnly => libc::PROT_READ,
                FinalProtection::ReadWrite => libc::PROT_READ | libc::PROT_WRITE,
            };
            if unsafe { libc::mprotect(address as *mut libc::c_void, len, protection) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) unsafe fn release(address: usize, len: usize) -> io::Result<()> {
            if unsafe { libc::munmap(address as *mut libc::c_void, len) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    #[cfg(windows)]
    mod platform {
        use super::{FinalProtection, io, ptr};

        const MEM_COMMIT: u32 = 0x1000;
        const MEM_RESERVE: u32 = 0x2000;
        const MEM_RELEASE: u32 = 0x8000;
        const PAGE_NOACCESS: u32 = 0x01;
        const PAGE_READONLY: u32 = 0x02;
        const PAGE_READWRITE: u32 = 0x04;
        const PAGE_EXECUTE_READ: u32 = 0x20;

        unsafe extern "system" {
            fn VirtualAlloc(
                address: *mut core::ffi::c_void,
                size: usize,
                allocation_type: u32,
                protect: u32,
            ) -> *mut core::ffi::c_void;
            fn VirtualProtect(address: *mut core::ffi::c_void, size: usize, protect: u32, old_protect: *mut u32)
            -> i32;
            fn VirtualFree(address: *mut core::ffi::c_void, size: usize, free_type: u32) -> i32;
            fn GetSystemInfo(system_info: *mut SystemInfo);
        }

        #[repr(C)]
        struct SystemInfo {
            processor_architecture: u16,
            reserved: u16,
            page_size: u32,
            minimum_application_address: *mut core::ffi::c_void,
            maximum_application_address: *mut core::ffi::c_void,
            active_processor_mask: usize,
            number_of_processors: u32,
            processor_type: u32,
            allocation_granularity: u32,
            processor_level: u16,
            processor_revision: u16,
        }

        pub(super) fn page_size() -> io::Result<usize> {
            let mut system_info = unsafe { core::mem::zeroed::<SystemInfo>() };
            unsafe { GetSystemInfo(&mut system_info) };
            usize::try_from(system_info.page_size).map_err(|_| io::Error::other("Windows page size does not fit usize"))
        }

        pub(super) fn reserve(len: usize) -> io::Result<usize> {
            let address = unsafe { VirtualAlloc(ptr::null_mut(), len, MEM_RESERVE, PAGE_NOACCESS) };
            if address.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(address.addr())
        }

        pub(super) unsafe fn make_writable(address: usize, len: usize) -> io::Result<()> {
            let committed = unsafe { VirtualAlloc(address as *mut core::ffi::c_void, len, MEM_COMMIT, PAGE_READWRITE) };
            if committed.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) unsafe fn protect(address: usize, len: usize, protection: FinalProtection) -> io::Result<()> {
            let protection = match protection {
                FinalProtection::ReadExecute => PAGE_EXECUTE_READ,
                FinalProtection::ReadOnly => PAGE_READONLY,
                FinalProtection::ReadWrite => PAGE_READWRITE,
            };
            let mut old_protection = 0;
            if unsafe { VirtualProtect(address as *mut core::ffi::c_void, len, protection, &mut old_protection) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) unsafe fn release(address: usize, _len: usize) -> io::Result<()> {
            if unsafe { VirtualFree(address as *mut core::ffi::c_void, 0, MEM_RELEASE) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod native_jit_settings_tests {
    use super::*;
    use cranelift_codegen::isa;

    fn policy_isa(triple: &str) -> std::sync::Arc<dyn isa::TargetIsa> {
        let builder = isa::lookup(triple.parse().expect("valid target triple")).expect("supported target ISA");
        native_jit_isa_from_builder(builder).expect("construct target-derived native JIT ISA")
    }

    #[test]
    fn native_jit_isa_policy_is_target_derived_and_preserves_shared_invariants() {
        let x86_64 = policy_isa("x86_64-unknown-linux-gnu");
        assert!(x86_64.flags().is_pic(), "x86_64 native JIT must materialize symbols through PIC/GOT");
        assert!(!x86_64.flags().use_colocated_libcalls(), "native JIT must use range-independent libcalls");
        assert!(x86_64.flags().preserve_frame_pointers(), "native JIT preserves the shared frame-pointer invariant");

        let aarch64 = policy_isa("aarch64-unknown-linux-gnu");
        assert!(!aarch64.flags().is_pic(), "non-x86 native JIT must not inherit the x86_64 PIC exception");
        assert!(!aarch64.flags().use_colocated_libcalls(), "native JIT must use range-independent libcalls");
        assert!(aarch64.flags().preserve_frame_pointers(), "native JIT preserves the shared frame-pointer invariant");
    }
}
