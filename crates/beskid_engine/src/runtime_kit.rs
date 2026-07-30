//! Exact ABI-v5 shared-runtime loading for JIT modules.

#[cfg(unix)]
use std::ffi::CStr;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{BuildProfile, RuntimeKitMetadata};
use beskid_abi::runtime_source::resolve_canonical_runtime_kit;

/// Manifest-owned record a host reserves for one activated ABI-v5 runtime.
const RUNTIME_STATE_LAYOUT: &str = "BeskidRuntimeState";
/// ABI-v5 embedder entrypoints that activate runtime state and attach the executing thread.
const PROCESS_INIT_EXPORT: &str = "beskid_rt_v5_process_init";
const PROCESS_SHUTDOWN_EXPORT: &str = "beskid_rt_v5_process_shutdown";
const THREAD_ATTACH_EXPORT: &str = "beskid_rt_v5_thread_attach";

/// Loaded shared runtime plus the exact metadata-approved JIT symbol map.
pub struct JitRuntimeKit {
    _library: DynamicLibrary,
    metadata: RuntimeKitMetadata,
    shared_library: PathBuf,
    symbols: Vec<(String, *const u8)>,
}

impl JitRuntimeKit {
    pub fn load(prefix: &Path, target: &TargetMetadata, profile: BuildProfile) -> Result<Self, String> {
        let kit = resolve_canonical_runtime_kit(prefix, target, profile)
            .map_err(|error| format!("ABI-v5 runtime kit validation failed: {error:?}"))?;
        let library = DynamicLibrary::open(&kit.shared_library)?;
        let mut symbols = Vec::with_capacity(kit.metadata.loader_required_exports.len());
        for name in &kit.metadata.loader_required_exports {
            symbols.push((name.clone(), library.symbol(name)?));
        }
        Ok(Self { _library: library, metadata: kit.metadata, shared_library: kit.shared_library, symbols })
    }

    pub fn metadata(&self) -> &RuntimeKitMetadata {
        &self.metadata
    }

    pub fn shared_library_path(&self) -> &Path {
        &self.shared_library
    }

    pub fn symbols(&self) -> &[(String, *const u8)] {
        &self.symbols
    }

    pub fn symbol_names(&self) -> impl Iterator<Item = &str> {
        self.symbols.iter().map(|(name, _)| name.as_str())
    }

    fn loader_required_symbol(&self, name: &str) -> Result<*const u8, String> {
        self.symbols
            .iter()
            .find(|(symbol, _)| symbol == name)
            .map(|(_, address)| *address)
            .ok_or_else(|| format!("ABI-v5 runtime kit does not export the required loader symbol `{name}`"))
    }
}

/// One caller-owned `BeskidRuntimeState` activated for the thread that runs JIT'd code.
///
/// `beskid_runtime_abi_v5.h` makes runtime-state ownership a host responsibility: the embedder
/// reserves the manifest-layout record, stamps it through `beskid_rt_v5_process_init`, and attaches
/// the executing thread with `beskid_rt_v5_thread_attach` before generated code reaches scheduler,
/// heap, or root-frame state. The in-process JIT is such a host, so the engine performs that
/// activation itself; without it every kit path that reads `RuntimeState()` fails closed with the
/// ABI-v5 `runtime_internal_corruption` trap.
pub(crate) struct AttachedRuntimeState {
    state: *mut u8,
    layout: std::alloc::Layout,
    shutdown: unsafe extern "C" fn(*mut u8),
}

impl AttachedRuntimeState {
    /// Reserve, initialize, and attach runtime state through the loaded kit's own exports.
    pub(crate) fn attach(kit: &JitRuntimeKit) -> Result<Self, String> {
        let record = kit
            .metadata()
            .abi_contract
            .layouts
            .iter()
            .find(|layout| layout.name == RUNTIME_STATE_LAYOUT)
            .ok_or_else(|| format!("ABI-v5 manifest is missing the `{RUNTIME_STATE_LAYOUT}` layout"))?;
        let (size, alignment) = (usize::try_from(record.size), usize::try_from(record.alignment));
        let (Ok(size), Ok(alignment)) = (size, alignment) else {
            return Err(format!("`{RUNTIME_STATE_LAYOUT}` layout exceeds the host address width"));
        };
        if size == 0 {
            return Err(format!("`{RUNTIME_STATE_LAYOUT}` layout must reserve a non-empty record"));
        }
        let layout = std::alloc::Layout::from_size_align(size, alignment)
            .map_err(|error| format!("invalid `{RUNTIME_STATE_LAYOUT}` layout: {error}"))?;

        let initialize = kit.loader_required_symbol(PROCESS_INIT_EXPORT)?;
        let attach = kit.loader_required_symbol(THREAD_ATTACH_EXPORT)?;
        let shutdown = kit.loader_required_symbol(PROCESS_SHUTDOWN_EXPORT)?;
        // SAFETY: the three addresses come from the kit's validated loader-required exports, whose
        // ABI-v5 signatures are frozen by the manifest this kit was audited against.
        let (initialize, attach, shutdown) = unsafe {
            (
                std::mem::transmute::<*const u8, unsafe extern "C" fn(*mut u8) -> *mut u8>(initialize),
                std::mem::transmute::<*const u8, unsafe extern "C" fn(*mut u8) -> *mut u8>(attach),
                std::mem::transmute::<*const u8, unsafe extern "C" fn(*mut u8)>(shutdown),
            )
        };

        // SAFETY: `layout` has a non-zero size taken from the validated manifest record.
        let state = unsafe { std::alloc::alloc_zeroed(layout) };
        if state.is_null() {
            return Err(format!("failed to reserve a {size}-byte `{RUNTIME_STATE_LAYOUT}` record"));
        }
        // SAFETY: `state` is a zeroed, correctly aligned record of exactly the manifest size, and
        // stays owned by this value until `Drop` runs the paired shutdown.
        let attached = unsafe {
            initialize(state);
            attach(state)
        };
        if attached.is_null() {
            // SAFETY: the failed attach installed no thread state, so the reservation is unshared.
            unsafe { std::alloc::dealloc(state, layout) };
            return Err("ABI-v5 runtime kit failed to attach the current thread".into());
        }
        Ok(Self { state, layout, shutdown })
    }
}

impl Drop for AttachedRuntimeState {
    fn drop(&mut self) {
        // SAFETY: shutdown detaches the thread state installed by `attach` before the reservation
        // is released, and the loaded kit outlives this value.
        unsafe {
            (self.shutdown)(self.state);
            std::alloc::dealloc(self.state, self.layout);
        }
    }
}

#[cfg(unix)]
struct DynamicLibrary(*mut std::ffi::c_void);

#[cfg(unix)]
impl DynamicLibrary {
    fn open(path: &Path) -> Result<Self, String> {
        use std::os::unix::ffi::OsStrExt;

        const RTLD_NOW: std::ffi::c_int = 2;
        // `RTLD_LOCAL` is the default on glibc, where its value is zero.  Do not use the
        // Darwin flag value here: Linux assigns bit 4 to `RTLD_NOLOAD`, which would reject a
        // fresh runtime kit instead of loading it.
        #[cfg(target_os = "linux")]
        const RTLD_LOCAL: std::ffi::c_int = 0;
        #[cfg(not(target_os = "linux"))]
        const RTLD_LOCAL: std::ffi::c_int = 4;
        unsafe extern "C" {
            fn dlopen(filename: *const std::ffi::c_char, flags: std::ffi::c_int) -> *mut std::ffi::c_void;
            fn dlerror() -> *const std::ffi::c_char;
        }
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| format!("runtime library path contains NUL: `{}`", path.display()))?;
        let display_path = path.to_string_lossy().into_owned();
        // POSIX requires clearing a prior loader diagnostic before observing the result of a new
        // dlopen call. Without this, an otherwise actionable ELF/TLS loader failure can surface
        // as an unhelpful empty diagnostic in hosted CI.
        unsafe {
            let _ = dlerror();
        }
        let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handle.is_null() {
            let diagnostic = last_dl_error(unsafe { dlerror() });
            let os_error = std::io::Error::last_os_error();
            return Err(format!(
                "dlopen `{display_path}` failed with RTLD_NOW|RTLD_LOCAL: {diagnostic}; errno={os_error}"
            ));
        }
        Ok(Self(handle))
    }

    fn symbol(&self, name: &str) -> Result<*const u8, String> {
        unsafe extern "C" {
            fn dlsym(handle: *mut std::ffi::c_void, symbol: *const std::ffi::c_char) -> *mut std::ffi::c_void;
            fn dlerror() -> *const std::ffi::c_char;
        }
        let symbol = CString::new(name).map_err(|_| format!("runtime symbol contains NUL: {name:?}"))?;
        unsafe {
            let _ = dlerror();
        }
        let address = unsafe { dlsym(self.0, symbol.as_ptr()) };
        let error = unsafe { dlerror() };
        if !error.is_null() || address.is_null() {
            return Err(format!("approved runtime export `{name}` is unavailable: {}", last_dl_error(error)));
        }
        Ok(address.cast())
    }
}

#[cfg(unix)]
impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn dlclose(handle: *mut std::ffi::c_void) -> std::ffi::c_int;
        }
        unsafe {
            let _ = dlclose(self.0);
        }
    }
}

#[cfg(unix)]
fn last_dl_error(error: *const std::ffi::c_char) -> String {
    if error.is_null() {
        "dynamic loader returned no diagnostic".into()
    } else {
        unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned()
    }
}

#[cfg(windows)]
struct DynamicLibrary(*mut std::ffi::c_void);

#[cfg(windows)]
impl DynamicLibrary {
    fn open(path: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn LoadLibraryW(path: *const u16) -> *mut std::ffi::c_void;
        }
        let wide = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<_>>();
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            return Err(format!("LoadLibraryW failed for `{}`", path.display()));
        }
        Ok(Self(handle))
    }

    fn symbol(&self, name: &str) -> Result<*const u8, String> {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
        }
        let name_c = CString::new(name).map_err(|_| format!("runtime symbol contains NUL: {name:?}"))?;
        let address = unsafe { GetProcAddress(self.0, name_c.as_ptr().cast()) };
        if address.is_null() {
            return Err(format!("approved runtime export `{name}` is unavailable"));
        }
        Ok(address.cast())
    }
}

#[cfg(windows)]
impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
