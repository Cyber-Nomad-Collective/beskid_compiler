use std::path::{Path, PathBuf};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_codegen::{CodegenArtifact, ExternImport};
use beskid_pipeline::PipelineObserver;

use crate::jit_module::{BeskidJitModule, JitError};
use crate::runtime_kit::AttachedRuntimeState;

#[derive(Clone)]
struct RuntimeKitSelection {
    prefix: PathBuf,
    target: TargetMetadata,
    profile: RuntimeKitProfile,
}

/// Owns an exact ABI-v5 runtime-kit selection and a [`BeskidJitModule`].
///
/// Field order is load-bearing: `runtime_state` detaches the calling thread and retires the heap
/// before `jit` releases the loaded runtime kit and artifact-owned descriptor storage.
pub struct Engine {
    runtime_kit: RuntimeKitSelection,
    runtime_state: Option<AttachedRuntimeState>,
    arguments_initialized: bool,
    jit: BeskidJitModule,
}

impl Engine {
    /// Build an engine from the exact ABI-v5 runtime kit installed with this executable.
    pub fn new() -> Self {
        Self::try_new().expect("failed to initialize exact ABI-v5 JIT runtime kit")
    }

    /// Fallible form of [`Self::new`].
    pub fn try_new() -> Result<Self, JitError> {
        let prefix = runtime_prefix()?;
        let target = host_runtime_target()?;
        let profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE") {
            Ok(value) => RuntimeKitProfile::parse(&value).map_err(|error| JitError::RuntimeKit(error.to_string()))?,
            Err(std::env::VarError::NotPresent) if cfg!(debug_assertions) => RuntimeKitProfile::Debug,
            Err(std::env::VarError::NotPresent) => RuntimeKitProfile::Release,
            Err(error) => return Err(JitError::RuntimeKit(format!("invalid BESKID_RUNTIME_KIT_PROFILE: {error}"))),
        };
        Self::with_runtime_kit(&prefix, target, profile)
    }

    /// Build an engine from one explicit, validated ABI-v5 runtime kit.
    pub fn with_runtime_kit(
        prefix: &Path,
        target: TargetMetadata,
        profile: RuntimeKitProfile,
    ) -> Result<Self, JitError> {
        let jit = BeskidJitModule::new_with_runtime_kit(prefix, &target, profile, &[], &[])?;
        // JIT'd code executes in this process against the kit just loaded, so the engine is the
        // ABI-v5 host: it owns the runtime-state reservation and the thread attachment that
        // scheduler, heap, and root-frame access require. Attaching here keeps that activation
        // alive across the per-artifact module rebuilds below, which never unload the kit.
        let runtime_state = AttachedRuntimeState::attach(jit.runtime_kit()).map_err(JitError::RuntimeKit)?;
        Ok(Self {
            runtime_kit: RuntimeKitSelection { prefix: prefix.to_path_buf(), target, profile },
            runtime_state: Some(runtime_state),
            arguments_initialized: false,
            jit,
        })
    }

    /// Initialize Core.Args once from an explicit host-owned vector.
    pub fn initialize_arguments(&mut self, arguments: &[String]) -> Result<(), JitError> {
        if self.arguments_initialized {
            return Err(JitError::RuntimeKit("Core.Args was already initialized for this engine".to_owned()));
        }
        self.jit.runtime_kit().handoff_arguments(&self.runtime_kit.target, arguments).map_err(JitError::RuntimeKit)?;
        self.arguments_initialized = true;
        Ok(())
    }

    /// Exact ABI-v5 target selected by this engine's validated runtime kit.
    pub fn target_metadata(&self) -> &TargetMetadata {
        &self.runtime_kit.target
    }

    /// Drop the current JIT module and reload the same validated exact runtime kit.
    pub fn reload_runtime_kit(&mut self) -> Result<(), JitError> {
        let jit = BeskidJitModule::new_with_runtime_kit(
            &self.runtime_kit.prefix,
            &self.runtime_kit.target,
            self.runtime_kit.profile,
            &[],
            &[],
        )?;
        self.replace_jit_and_runtime(jit)
    }

    /// Load `artifact` into a fresh or reused JIT module, declare builtins/externs, define functions, finalize.
    pub fn compile_artifact(&mut self, artifact: &CodegenArtifact) -> Result<(), JitError> {
        self.compile_artifact_with_pipeline(artifact, None)
    }

    /// Same as [`Self::compile_artifact`], emitting [`beskid_pipeline::phases::JIT_EMIT`] / [`beskid_pipeline::phases::JIT_FINALIZE`] work when `pipeline` is set.
    pub fn compile_artifact_with_pipeline(
        &mut self,
        artifact: &CodegenArtifact,
        pipeline: Option<&dyn PipelineObserver>,
    ) -> Result<(), JitError> {
        if requires_explicit_jit_arguments(artifact) && !self.arguments_initialized {
            return Err(JitError::Isa("Core.Args requires explicit JIT arguments".to_owned()));
        }
        // `trusted_extern_imports` republishes (not replaces) the subset of `extern_imports` the
        // compiler itself resolved through the canonical runtime intrinsic capability (see
        // `CodegenArtifact::trusted_extern_imports`); a name appearing there can never have come
        // from a source-level `[Extern]` declaration. Exclude that subset from the generic
        // user-FFI path below — it is resolved separately, directly from this exact kit's shared
        // library — rather than folding it into `kit_exports`, which stays the closed set covered
        // by `loader_required_exports` (see `is_exact_runtime_symbol`).
        let trusted_names: std::collections::HashSet<&str> =
            artifact.trusted_extern_imports.iter().map(|import| import.symbol.as_str()).collect();
        let referenced = beskid_codegen::referenced_extern_imports(artifact);
        let binds_absent_sentinel =
            referenced.iter().any(|entry| entry.symbol == beskid_codegen::OPTIONAL_EXTERN_ABSENT_SYMBOL);
        let user_ffi_imports = referenced
            .into_iter()
            .filter(|entry| {
                !self.jit.is_exact_runtime_symbol(&entry.symbol)
                    && !trusted_names.contains(entry.symbol.as_str())
                    && entry.symbol != beskid_codegen::OPTIONAL_EXTERN_ABSENT_SYMBOL
            })
            .collect::<Vec<_>>();
        validate_authorized_user_ffi(&user_ffi_imports)?;
        let (optional_ffi_imports, required_ffi_imports): (Vec<_>, Vec<_>) =
            user_ffi_imports.into_iter().partition(|import| import.optional);

        #[cfg(unix)]
        let mut authorized_user_ffi = if required_ffi_imports.is_empty() {
            Vec::new()
        } else {
            resolve_process_extern_symbols(&required_ffi_imports)
                .map_err(|e| JitError::Isa(format!("extern resolve: {}", e)))?
        };

        #[cfg(not(unix))]
        let mut authorized_user_ffi: Vec<(String, *const u8)> = {
            if !required_ffi_imports.is_empty() {
                let list = required_ffi_imports.iter().map(|e| e.symbol.clone()).collect::<Vec<_>>().join(", ");
                return Err(JitError::Isa(format!(
                    "extern imports present but JIT extern resolution is unsupported on this host: {}",
                    list
                )));
            }
            Vec::new()
        };

        // An optional contract's symbol is resolved best-effort, one at a time: a missing library
        // or symbol binds the name to the absent sentinel, which generated thunks and
        // `Available()` check. (cranelift-jit cannot relocate a null target through the x86-64
        // GOT, so null is not used.)
        let absent = optional_extern_absent as *const u8;
        if binds_absent_sentinel {
            authorized_user_ffi.push((beskid_codegen::OPTIONAL_EXTERN_ABSENT_SYMBOL.to_owned(), absent));
        }
        for import in &optional_ffi_imports {
            #[cfg(unix)]
            let resolved = resolve_process_extern_symbols(std::slice::from_ref(import)).ok();
            #[cfg(not(unix))]
            let resolved: Option<Vec<(String, *const u8)>> = None;
            let address =
                resolved.and_then(|resolved| resolved.into_iter().next()).map_or(absent, |(_, address)| address);
            authorized_user_ffi.push((import.symbol.clone(), address));
        }

        // `trusted_extern_imports` carries only imports the compiler itself resolved through the
        // canonical runtime intrinsic capability (see `CodegenArtifact::trusted_extern_imports`),
        // never a source-level `[Extern]` declaration. Resolve each one straight from this exact
        // kit's own shared library: their provenance is already proven by which artifact field
        // they arrived in, so this does not widen `is_exact_runtime_symbol`/`kit_exports`, which
        // stays the closed set covered by `loader_required_exports`.
        let trusted_intrinsic_ffi = beskid_codegen::referenced_trusted_extern_imports(artifact)
            .iter()
            .map(|import| {
                self.jit
                    .runtime_kit()
                    .resolve_trusted_symbol(&import.symbol)
                    .map(|address| (import.symbol.clone(), address))
                    .map_err(JitError::RuntimeKit)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Recreate the module per artifact while preserving the exact runtime-kit authority.
        let jit = BeskidJitModule::new_with_runtime_kit(
            &self.runtime_kit.prefix,
            &self.runtime_kit.target,
            self.runtime_kit.profile,
            &authorized_user_ffi,
            &trusted_intrinsic_ffi,
        )?;
        self.replace_jit_and_runtime(jit)?;

        self.jit.compile_with_pipeline(artifact, pipeline)
    }

    /// Replace one artifact-owned JIT only after retiring the heap that can reference its static
    /// descriptors, then attach a fresh runtime state to the replacement module's exact kit.
    fn replace_jit_and_runtime(&mut self, jit: BeskidJitModule) -> Result<(), JitError> {
        drop(self.runtime_state.take());
        self.jit = jit;
        self.runtime_state = Some(AttachedRuntimeState::attach(self.jit.runtime_kit()).map_err(JitError::RuntimeKit)?);
        Ok(())
    }

    /// Resolved machine code for `name` after successful compile; caller must match the real signature.
    ///
    /// # Safety
    ///
    /// The caller must cast the returned pointer to the exact generated function signature and
    /// must only call it while the owning JIT module remains alive.
    pub unsafe fn entrypoint_ptr(&mut self, name: &str) -> Result<*const u8, JitError> {
        let func_id = self.jit.get_func_id(name).ok_or_else(|| JitError::MissingFunction(name.to_string()))?;
        Ok(unsafe { self.jit.get_finalized_function_ptr(func_id) })
    }

    #[doc(hidden)]
    pub fn jit_module_mut(&mut self) -> &mut cranelift_jit::JITModule {
        self.jit.module()
    }
}

/// Address bound to an optional `[Extern]` symbol that did not resolve. Generated code compares
/// against it and never calls it; reaching it means a thunk was bypassed.
extern "C" fn optional_extern_absent() {
    std::process::abort();
}

fn validate_authorized_user_ffi(imports: &[ExternImport]) -> Result<(), JitError> {
    for import in imports {
        if beskid_abi::is_runtime_owned_ffi_symbol(&import.symbol) {
            return Err(JitError::RuntimeKit(format!(
                "runtime/host symbol `{}` must be supplied by the selected exact ABI-v5 runtime kit, not user FFI",
                import.symbol
            )));
        }
        if import.abi.as_deref() != Some("C") || import.library.as_deref().is_none_or(str::is_empty) {
            return Err(JitError::Isa(format!(
                "user FFI symbol `{}` requires an explicit C ABI and library authority",
                import.symbol
            )));
        }
    }
    Ok(())
}

fn requires_explicit_jit_arguments(artifact: &CodegenArtifact) -> bool {
    artifact.extern_imports.iter().any(|import| {
        ABI_V5_CORELIB_SERVICE_BINDINGS
            .iter()
            .any(|binding| binding.service.starts_with("__args_") && binding.adapter == import.symbol)
    })
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn runtime_prefix() -> Result<PathBuf, JitError> {
    beskid_abi::runtime_kit::installed_runtime_prefix().map_err(|error| JitError::RuntimeKit(error.to_string()))
}

/// ABI-v5 target metadata for the native JIT host.
pub fn host_runtime_target() -> Result<TargetMetadata, JitError> {
    beskid_abi::runtime_kit::host_runtime_target().map_err(|error| JitError::RuntimeKit(error.to_string()))
}

/// Resolve extern symbols from libraries already mapped into this process (libc, pthread, …).
///
/// Used for JIT runs on Unix hosts where the dynamic linker has already loaded standard libraries;
/// a symbol the process lacks is looked up in the import's declared library.
#[cfg(unix)]
fn resolve_process_extern_symbols(imports: &[ExternImport]) -> Result<Vec<(String, *const u8)>, String> {
    use std::ffi::{CStr, CString};

    let mut result = Vec::with_capacity(imports.len());
    for imp in imports {
        let c_sym = CString::new(imp.symbol.as_str()).map_err(|_| format!("bad symbol: {}", imp.symbol))?;
        // POSIX requires callers to clear the thread-local error before dlsym.
        // libc also supplies the platform-correct RTLD_DEFAULT value: null on
        // glibc, but -2 on Darwin.
        unsafe { libc::dlerror() };
        let mut addr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c_sym.as_ptr()) };
        // Cranelift's Mach-O import spelling carries the object-file leading
        // underscore, whereas dlsym expects the C source name.
        #[cfg(target_os = "macos")]
        if addr.is_null()
            && let Some(symbol) = imp.symbol.strip_prefix('_')
        {
            let c_symbol = CString::new(symbol).map_err(|_| format!("bad symbol: {symbol}"))?;
            unsafe { libc::dlerror() };
            addr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c_symbol.as_ptr()) };
        }
        // A symbol the process has not loaded comes from the `[Extern]` contract's library, as
        // an AOT link of the same program would arrange. The handle stays open for the life of
        // the process because finalized JIT code keeps the address.
        if addr.is_null()
            && let Some(library) = imp.library.as_deref().filter(|library| !library.is_empty())
        {
            let c_library = CString::new(library).map_err(|_| format!("bad library: {library}"))?;
            unsafe { libc::dlerror() };
            let handle = unsafe { libc::dlopen(c_library.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            if handle.is_null() {
                let error = unsafe { libc::dlerror() };
                let detail = if error.is_null() {
                    "library could not be loaded".into()
                } else {
                    unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned()
                };
                return Err(format!("dlopen({library}) for {}: {detail}", imp.symbol));
            }
            unsafe { libc::dlerror() };
            addr = unsafe { libc::dlsym(handle, c_sym.as_ptr()) };
        }
        if addr.is_null() {
            let error = unsafe { libc::dlerror() };
            let detail = if error.is_null() {
                "symbol resolved to a null address".into()
            } else {
                unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned()
            };
            return Err(format!("dlsym({}): {detail}", imp.symbol));
        }
        result.push((imp.symbol.clone(), addr as *const u8));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::requires_explicit_jit_arguments;
    use beskid_codegen::{CodegenArtifact, ExternImport};

    #[test]
    fn core_args_has_no_ambient_jit_zero_vector() {
        let artifact = CodegenArtifact {
            extern_imports: vec![ExternImport {
                symbol: "beskid_rt_v5_args_count".into(),
                abi: Some("C".into()),
                library: None,
                optional: false,
            }],
            ..Default::default()
        };
        assert!(requires_explicit_jit_arguments(&artifact));
    }
}
