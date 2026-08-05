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
/// Field order is load-bearing: `runtime_state` detaches the calling thread before `jit` releases
/// the loaded runtime kit that owns those exports.
pub struct Engine {
    runtime_kit: RuntimeKitSelection,
    _runtime_state: AttachedRuntimeState,
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
        let profile = if cfg!(debug_assertions) { RuntimeKitProfile::Debug } else { RuntimeKitProfile::Release };
        Self::with_runtime_kit(&prefix, target, profile)
    }

    /// Build an engine from one explicit, validated ABI-v5 runtime kit.
    pub fn with_runtime_kit(
        prefix: &Path,
        target: TargetMetadata,
        profile: RuntimeKitProfile,
    ) -> Result<Self, JitError> {
        let jit = BeskidJitModule::new_with_runtime_kit(prefix, &target, profile, &[])?;
        // JIT'd code executes in this process against the kit just loaded, so the engine is the
        // ABI-v5 host: it owns the runtime-state reservation and the thread attachment that
        // scheduler, heap, and root-frame access require. Attaching here keeps that activation
        // alive across the per-artifact module rebuilds below, which never unload the kit.
        let runtime_state = AttachedRuntimeState::attach(jit.runtime_kit()).map_err(JitError::RuntimeKit)?;
        Ok(Self {
            runtime_kit: RuntimeKitSelection { prefix: prefix.to_path_buf(), target, profile },
            _runtime_state: runtime_state,
            jit,
        })
    }

    /// Exact ABI-v5 target selected by this engine's validated runtime kit.
    pub fn target_metadata(&self) -> &TargetMetadata {
        &self.runtime_kit.target
    }

    /// Drop the current JIT module and reload the same validated exact runtime kit.
    pub fn reload_runtime_kit(&mut self) -> Result<(), JitError> {
        self.jit = BeskidJitModule::new_with_runtime_kit(
            &self.runtime_kit.prefix,
            &self.runtime_kit.target,
            self.runtime_kit.profile,
            &[],
        )?;
        Ok(())
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
        if requires_explicit_jit_arguments(artifact) {
            return Err(JitError::Isa("Core.Args requires explicit JIT arguments".to_owned()));
        }
        let runtime_externs = beskid_codegen::referenced_extern_imports(artifact)
            .into_iter()
            .filter(|entry| !self.jit.is_exact_runtime_symbol(&entry.symbol))
            .collect::<Vec<_>>();

        #[cfg(feature = "extern_dlopen")]
        let extras =
            resolve_extern_symbols(&runtime_externs).map_err(|e| JitError::Isa(format!("extern resolve: {}", e)))?;

        #[cfg(all(not(feature = "extern_dlopen"), unix))]
        let extras = if runtime_externs.is_empty() {
            Vec::new()
        } else {
            resolve_process_extern_symbols(&runtime_externs)
                .map_err(|e| JitError::Isa(format!("extern resolve: {}", e)))?
        };

        #[cfg(all(not(feature = "extern_dlopen"), not(unix)))]
        let extras: Vec<(String, *const u8)> = {
            if !runtime_externs.is_empty() {
                let list = runtime_externs.iter().map(|e| e.symbol.clone()).collect::<Vec<_>>().join(", ");
                return Err(JitError::Isa(format!(
                    "extern imports present but JIT extern resolution is unsupported on this host: {}",
                    list
                )));
            }
            Vec::new()
        };

        // Recreate the module per artifact while preserving the exact runtime-kit authority.
        self.jit = BeskidJitModule::new_with_runtime_kit(
            &self.runtime_kit.prefix,
            &self.runtime_kit.target,
            self.runtime_kit.profile,
            &extras,
        )?;

        #[cfg(debug_assertions)]
        {
            if let Err(missing) = beskid_codegen::validate_artifact(artifact) {
                let names: Vec<_> = missing.iter().map(|m| m.name.as_str()).collect();
                return Err(JitError::Isa(format!(
                    "codegen artifact validation failed: undefined callees: {}",
                    names.join(", ")
                )));
            }
        }

        self.jit.compile_with_pipeline(artifact, pipeline)
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

fn requires_explicit_jit_arguments(artifact: &CodegenArtifact) -> bool {
    artifact.extern_imports.iter().any(|import| {
        ABI_V5_CORELIB_SERVICE_BINDINGS
            .iter()
            .any(|binding| binding.service.starts_with("__args_") && binding.adapter == import.symbol)
    })
}

#[cfg(test)]
mod tests {
    use super::requires_explicit_jit_arguments;
    use beskid_codegen::{CodegenArtifact, ExternImport};

    #[test]
    fn core_args_has_no_ambient_jit_zero_vector() {
        let artifact = CodegenArtifact {
            extern_imports: vec![ExternImport { symbol: "beskid_rt_v5_args_count".into(), abi: Some("C".into()), library: None }],
            ..Default::default()
        };
        assert!(requires_explicit_jit_arguments(&artifact));
    }
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
/// Used for JIT runs on Unix hosts where the dynamic linker has already loaded standard libraries.
#[cfg(all(not(feature = "extern_dlopen"), unix))]
fn resolve_process_extern_symbols(imports: &[ExternImport]) -> Result<Vec<(String, *const u8)>, String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_void};

    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }

    let mut result = Vec::with_capacity(imports.len());
    for imp in imports {
        let c_sym = CString::new(imp.symbol.as_str()).map_err(|_| format!("bad symbol: {}", imp.symbol))?;
        let mut addr = unsafe { dlsym(RTLD_DEFAULT, c_sym.as_ptr()) };
        // Cranelift's Mach-O import spelling carries the object-file leading
        // underscore, whereas dlsym expects the C source name.
        #[cfg(target_os = "macos")]
        if addr.is_null()
            && let Some(symbol) = imp.symbol.strip_prefix('_')
        {
            let c_symbol = CString::new(symbol).map_err(|_| format!("bad symbol: {symbol}"))?;
            addr = unsafe { dlsym(RTLD_DEFAULT, c_symbol.as_ptr()) };
        }
        if addr.is_null() {
            let err = unsafe { CStr::from_ptr(dlerror()) };
            return Err(format!("dlsym({}): {}", imp.symbol, err.to_string_lossy()));
        }
        result.push((imp.symbol.clone(), addr as *const u8));
    }
    Ok(result)
}

#[cfg(feature = "extern_dlopen")]
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "extern_dlopen")]
struct ExternCaches {
    libs: Mutex<std::collections::HashMap<String, usize>>, // handle as usize
    symbols: Mutex<std::collections::HashMap<(String, String), usize>>, // addr as usize
}

#[cfg(feature = "extern_dlopen")]
static EXTERN_CACHES: OnceLock<ExternCaches> = OnceLock::new();

#[cfg(feature = "extern_dlopen")]
static SECURITY_TEST: OnceLock<Mutex<(Option<Vec<String>>, Option<Vec<String>>)>> = OnceLock::new();

#[cfg(feature = "extern_dlopen")]
fn caches() -> &'static ExternCaches {
    EXTERN_CACHES
        .get_or_init(|| ExternCaches { libs: Mutex::new(Default::default()), symbols: Mutex::new(Default::default()) })
}

#[cfg(feature = "extern_dlopen")]
fn resolve_extern_symbols(imports: &[ExternImport]) -> Result<Vec<(String, *const u8)>, String> {
    // no local imports
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};

    const RTLD_NOW: c_int = 2;
    // Keep the external resolver's Linux flags aligned with the runtime-kit loader: glibc
    // defines RTLD_LOCAL as zero, while bit 4 is RTLD_NOLOAD.
    #[cfg(target_os = "linux")]
    const RTLD_LOCAL: c_int = 0;
    #[cfg(not(target_os = "linux"))]
    const RTLD_LOCAL: c_int = 4;

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }

    let mut result = Vec::new();

    // Optional security controls via environment variables:
    // BESKID_EXTERN_ALLOW: comma-separated patterns; if present, only matches are allowed
    // BESKID_EXTERN_DENY:  comma-separated patterns; matches are denied
    // Pattern forms: "lib:symbol", "lib:*", "*:symbol", or just "symbol". '*' is a wildcard.
    let (allow_pats, deny_pats): (Vec<String>, Vec<String>) = if let Some(m) = SECURITY_TEST.get() {
        let guard = m.lock().map_err(|_| "extern security cache poisoned".to_string())?;
        (guard.0.clone().unwrap_or_default(), guard.1.clone().unwrap_or_default())
    } else {
        let allow = std::env::var("BESKID_EXTERN_ALLOW").ok();
        let deny = std::env::var("BESKID_EXTERN_DENY").ok();
        let parse = |s: Option<String>| -> Vec<String> {
            s.as_deref()
                .map(|s| s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default()
        };
        (parse(allow), parse(deny))
    };

    fn pat_match(pat: &str, text: &str) -> bool {
        if pat == "*" {
            return true;
        }
        if let Some(idx) = pat.find('*') {
            let (pre, post) = pat.split_at(idx);
            let post = &post[1..]; // drop '*'
            return text.starts_with(pre) && text.ends_with(post);
        }
        pat == text
    }
    fn allow_deny_check(allow: &[String], deny: &[String], lib: &str, sym: &str) -> Result<(), String> {
        let matches_pat = |p: &str| -> bool {
            if let Some(colon) = p.find(':') {
                let (lp, sp) = p.split_at(colon);
                let sp = &sp[1..];
                pat_match(lp, lib) && pat_match(sp, sym)
            } else {
                pat_match(p, sym)
            }
        };
        if !allow.is_empty() && !allow.iter().any(|p| matches_pat(p)) {
            return Err(format!("extern {}:{} denied by allowlist", lib, sym));
        }
        if deny.iter().any(|p| matches_pat(p)) {
            return Err(format!("extern {}:{} denied by denylist", lib, sym));
        }
        Ok(())
    }
    let caches = caches();
    let mut libs_guard = caches.libs.lock().map_err(|_| "extern cache poisoned (libs)".to_string())?;
    let mut syms_guard = caches.symbols.lock().map_err(|_| "extern cache poisoned (symbols)".to_string())?;

    for imp in imports {
        let Some(lib) = imp.library.as_ref() else {
            return Err(format!("missing Library for extern symbol {}", imp.symbol));
        };
        let key = (lib.clone(), imp.symbol.clone());
        allow_deny_check(&allow_pats, &deny_pats, lib, &imp.symbol)?;
        if let Some(&addr) = syms_guard.get(&key) {
            result.push((imp.symbol.clone(), addr as *const u8));
            continue;
        }

        let handle = if let Some(&h) = libs_guard.get(lib) {
            h as *mut c_void
        } else {
            let c_lib = CString::new(lib.as_str()).map_err(|_| format!("bad library: {}", lib))?;
            let h = unsafe { dlopen(c_lib.as_ptr(), RTLD_LOCAL | RTLD_NOW) };
            if h.is_null() {
                let err = unsafe { CStr::from_ptr(dlerror()) };
                return Err(format!("dlopen({}): {}", lib, err.to_string_lossy()));
            }
            libs_guard.insert(lib.clone(), h as usize);
            h
        };

        let c_sym = CString::new(imp.symbol.as_str()).map_err(|_| format!("bad symbol: {}", imp.symbol))?;
        let addr = unsafe { dlsym(handle, c_sym.as_ptr()) };
        if addr.is_null() {
            let err = unsafe { CStr::from_ptr(dlerror()) };
            return Err(format!("dlsym({}): {}", imp.symbol, err.to_string_lossy()));
        }
        let addr_u8 = addr as *const u8;
        syms_guard.insert((lib.clone(), imp.symbol.clone()), addr_u8 as usize);
        result.push((imp.symbol.clone(), addr_u8));
    }

    Ok(result)
}

#[cfg(feature = "extern_dlopen")]
pub fn set_security_policies_for_tests(allow: Option<&str>, deny: Option<&str>) {
    let m = SECURITY_TEST.get_or_init(|| Mutex::new((None, None)));
    let mut guard = m.lock().unwrap();
    let parse = |s: Option<&str>| -> Option<Vec<String>> {
        s.map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
    };
    guard.0 = parse(allow);
    guard.1 = parse(deny);
}

/// For integration tests: resolve a small set of externs without compiling an artifact.
/// Each tuple is (symbol, library).
#[cfg(feature = "extern_dlopen")]
pub fn resolve_for_tests(requests: &[(&str, &str)]) -> Result<Vec<*const u8>, String> {
    let imports: Vec<ExternImport> = requests
        .iter()
        .map(|(sym, lib)| ExternImport {
            symbol: (*sym).to_string(),
            abi: Some("C".to_string()),
            library: Some((*lib).to_string()),
        })
        .collect();
    resolve_extern_symbols(&imports).map(|v| v.into_iter().map(|(_, p)| p).collect())
}
