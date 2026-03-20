use beskid_codegen::CodegenArtifact;
#[cfg(feature = "extern_dlopen")]
use beskid_codegen::ExternImport;
use beskid_runtime::{
    RuntimeRoot, RuntimeState, clear_current_mutation, clear_current_root, enter_runtime_scope,
    leave_runtime_scope, set_current_mutation, set_current_root,
};
use gc_arena::{Arena, DynamicRootSet, Mutation, Rootable};

use crate::jit_module::{BeskidJitModule, JitError};

type BeskidArena = Arena<Rootable![RuntimeRoot<'_>]>;

pub struct Engine {
    arena: BeskidArena,
    jit: BeskidJitModule,
}

impl Engine {
    pub fn new() -> Self {
        let arena = Arena::new(|mc| RuntimeRoot {
            globals: Vec::new(),
            dynamic_roots: DynamicRootSet::new(mc),
            runtime_state: RuntimeState::default(),
        });
        let jit = BeskidJitModule::new().expect("failed to initialize JIT module");
        Self { arena, jit }
    }

    pub fn compile_artifact(&mut self, artifact: &CodegenArtifact) -> Result<(), JitError> {
        #[cfg(feature = "extern_dlopen")]
        let extras = resolve_extern_symbols(&artifact.extern_imports)
            .map_err(|e| JitError::Isa(format!("extern resolve: {}", e)))?;

        #[cfg(not(feature = "extern_dlopen"))]
        let extras: Vec<(String, *const u8)> = {
            if !artifact.extern_imports.is_empty() {
                let list = artifact
                    .extern_imports
                    .iter()
                    .map(|e| e.symbol.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(JitError::Isa(format!(
                    "extern_dlopen feature disabled but extern imports present: {}",
                    list
                )));
            }
            Vec::new()
        };

        // Recreate JIT module per artifact to register extra symbols at builder time.
        self.jit = if extras.is_empty() {
            BeskidJitModule::new()?
        } else {
            BeskidJitModule::new_with_symbols(&extras)?
        };

        self.jit.compile(artifact)
    }

    pub unsafe fn entrypoint_ptr(&mut self, name: &str) -> Result<*const u8, JitError> {
        let func_id = self
            .jit
            .get_func_id(name)
            .ok_or_else(|| JitError::MissingFunction(name.to_string()))?;
        Ok(unsafe { self.jit.get_finalized_function_ptr(func_id) })
    }

    pub fn with_arena<R>(
        &mut self,
        f: impl for<'gc> FnOnce(&'gc Mutation<'gc>, &'gc mut RuntimeRoot<'gc>) -> R,
    ) -> R {
        self.arena.mutate_root(|mc, root| {
            enter_runtime_scope();
            set_current_mutation(mc as *const _ as *mut _);
            set_current_root(root as *mut _);
            struct Guard;
            impl Drop for Guard {
                fn drop(&mut self) {
                    clear_current_mutation();
                    clear_current_root();
                    leave_runtime_scope();
                }
            }
            let _guard = Guard;
            f(mc, root)
        })
    }

    #[doc(hidden)]
    pub fn jit_module_mut(&mut self) -> &mut cranelift_jit::JITModule {
        self.jit.module()
    }
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
    EXTERN_CACHES.get_or_init(|| ExternCaches {
        libs: Mutex::new(Default::default()),
        symbols: Mutex::new(Default::default()),
    })
}

#[cfg(feature = "extern_dlopen")]
fn resolve_extern_symbols(
    imports: &[ExternImport],
) -> Result<Vec<(String, *const u8)>, String> {
    // no local imports
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};

    const RTLD_NOW: c_int = 2;
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
        (
            guard.0.clone().unwrap_or_default(),
            guard.1.clone().unwrap_or_default(),
        )
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
        if pat == "*" { return true; }
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
    let mut libs_guard = caches
        .libs
        .lock()
        .map_err(|_| "extern cache poisoned (libs)".to_string())?;
    let mut syms_guard = caches
        .symbols
        .lock()
        .map_err(|_| "extern cache poisoned (symbols)".to_string())?;

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

        let c_sym = CString::new(imp.symbol.as_str())
            .map_err(|_| format!("bad symbol: {}", imp.symbol))?;
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
