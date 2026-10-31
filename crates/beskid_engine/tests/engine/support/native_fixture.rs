//! Engine-test-only loader for callbacks in native fixture libraries.

use std::{
    ffi::{CStr, c_void},
    path::Path,
};

pub struct NativeFixture<T: Copy> {
    library: *mut c_void,
    pub entry: T,
}

impl<T: Copy> NativeFixture<T> {
    /// The caller must supply the exact C function-pointer type exported by
    /// `symbol`, and keep this fixture alive while that pointer is used.
    pub unsafe fn load(path: &Path, symbol: &CStr) -> Self {
        assert_eq!(size_of::<T>(), size_of::<*mut c_void>(), "fixture entry must be a function pointer");
        let (library, entry) = unsafe { platform::load(path, symbol) };
        Self { library, entry: unsafe { std::mem::transmute_copy(&entry) } }
    }
}

impl<T: Copy> Drop for NativeFixture<T> {
    fn drop(&mut self) {
        unsafe { platform::close(self.library) };
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    pub unsafe fn load(path: &Path, symbol: &CStr) -> (*mut c_void, *mut c_void) {
        let encoded = CString::new(path.as_os_str().as_bytes()).unwrap();
        unsafe {
            let library = libc::dlopen(encoded.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if library.is_null() {
                panic!("dlopen {}: {}", path.display(), CStr::from_ptr(libc::dlerror()).to_string_lossy());
            }
            let entry = libc::dlsym(library, symbol.as_ptr());
            if entry.is_null() {
                libc::dlclose(library);
                panic!("{} missing from {}", symbol.to_string_lossy(), path.display());
            }
            (library, entry)
        }
    }

    pub unsafe fn close(library: *mut c_void) {
        unsafe {
            libc::dlclose(library);
        }
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
mod platform {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x00000100;
    const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x00001000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn GetLastError() -> u32;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    pub unsafe fn load(path: &Path, symbol: &CStr) -> (*mut c_void, *mut c_void) {
        assert!(path.is_absolute(), "fixture DLL path must be absolute: {}", path.display());
        let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            let library = LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            );
            assert!(!library.is_null(), "LoadLibraryExW {} failed: GetLastError={}", path.display(), GetLastError());
            let entry = GetProcAddress(library, symbol.as_ptr().cast());
            if entry.is_null() {
                let error = GetLastError();
                FreeLibrary(library);
                panic!(
                    "GetProcAddress {} in {} failed: GetLastError={error}",
                    symbol.to_string_lossy(),
                    path.display()
                );
            }
            (library, entry)
        }
    }

    pub unsafe fn close(library: *mut c_void) {
        unsafe {
            FreeLibrary(library);
        }
    }
}
