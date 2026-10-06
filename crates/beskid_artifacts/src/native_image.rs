//! Exact image integrity and handle lifetime. This module does not issue source,
//! ABI, semantic, or compiled-shape admission; callers must hold those witnesses.
use libloading::{Library, Symbol};
use sha2::{Digest, Sha256};
use std::{
    ffi::CStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum NativeImageError {
    #[error("native image {path}: {message}")]
    Integrity { path: PathBuf, message: String },
    #[error("load pinned native image {path}: {message}")]
    Load { path: PathBuf, message: String },
}
type Result<T> = std::result::Result<T, NativeImageError>;
fn invalid(path: &Path, message: impl ToString) -> NativeImageError {
    NativeImageError::Integrity { path: path.to_owned(), message: message.to_string() }
}
fn digest(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| invalid(path, error))?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 * 1024 {
        return Err(invalid(path, "requires a bounded regular image without a symlink"));
    }
    let mut file = fs::File::open(path).map_err(|error| invalid(path, error))?;
    let opened = file.metadata().map_err(|error| invalid(path, error))?;
    if !opened.is_file() || opened.len() != metadata.len() {
        return Err(invalid(path, "image changed while opening"));
    }
    let expected_length = opened.len();
    let mut consumed = 0u64;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| invalid(path, error))?;
        if count == 0 {
            break;
        }
        consumed = consumed.checked_add(count as u64).ok_or_else(|| invalid(path, "image length overflow"))?;
        if consumed > expected_length {
            return Err(invalid(path, "image grew while hashing"));
        }
        hasher.update(&buffer[..count]);
    }
    if consumed != expected_length || file.metadata().map_err(|error| invalid(path, error))?.len() != expected_length {
        return Err(invalid(path, "image length changed while hashing"));
    }
    Ok(format!("{:x}", hasher.finalize()))
}
struct PinnedImage {
    path: PathBuf,
    sha256: String,
}
impl PinnedImage {
    fn new(path: &Path, sha256: &str) -> Result<Self> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
            return Err(invalid(path, "invalid expected SHA256"));
        }
        let canonical = fs::canonicalize(path).map_err(|error| invalid(path, error))?;
        // The selected final path itself cannot be a mutable launch symlink.
        if fs::symlink_metadata(path).map_err(|error| invalid(path, error))?.file_type().is_symlink() {
            return Err(invalid(path, "native image symlink is forbidden"));
        }
        let image = Self { path: canonical, sha256: sha256.to_owned() };
        image.verify()?;
        Ok(image)
    }
    fn verify(&self) -> Result<()> {
        if digest(&self.path)? != self.sha256 {
            return Err(invalid(&self.path, "image changed from its qualified digest"));
        }
        Ok(())
    }
}

/// Image drops before provider. Borrowed symbol handles cannot outlive this set.
pub struct PinnedNativeImages {
    attached: Vec<AttachedImage>,
    image: Library,
    _provider: Library,
    image_pin: PinnedImage,
    provider_pin: PinnedImage,
    loaded_image: PathBuf,
    loaded_provider: PathBuf,
    _snapshot: tempfile::TempDir,
}
struct AttachedImage {
    image: Library,
    pin: PinnedImage,
    loaded: PathBuf,
    _snapshot: tempfile::TempDir,
}
impl PinnedNativeImages {
    /// Load the provider first, then the exact qualified image, checking both
    /// around loading. This proves integrity only, not caller source authority.
    ///
    /// # Safety
    /// The caller must hold private producer/loader admission for the provider,
    /// image and all initializer code. Loading can execute native initializers.
    pub unsafe fn load(provider: &Path, provider_sha256: &str, image: &Path, image_sha256: &str) -> Result<Self> {
        let provider_pin = PinnedImage::new(provider, provider_sha256)?;
        let image_pin = PinnedImage::new(image, image_sha256)?;
        // dlopen must consume private verified copies, not mutable producer paths.
        // Retain the private directory until both image handles have been dropped.
        let snapshot = tempfile::Builder::new()
            .prefix("beskid-native-images-")
            .tempdir()
            .map_err(|error| invalid(image, error))?;
        let loaded_provider = snapshot_image(&provider_pin, snapshot.path())?;
        let loaded_image = snapshot_image(&image_pin, snapshot.path())?;
        if loaded_provider == loaded_image {
            return Err(invalid(image, "provider and image names conflict"));
        }
        let provider = unsafe { open_image(&loaded_provider, true) }?;
        provider_pin.verify()?;
        let image = unsafe { open_image(&loaded_image, false) }?;
        provider_pin.verify()?;
        image_pin.verify()?;
        Ok(Self {
            attached: Vec::new(),
            image,
            _provider: provider,
            image_pin,
            provider_pin,
            loaded_image,
            loaded_provider,
            _snapshot: snapshot,
        })
    }
    pub fn verify_integrity(&self) -> Result<()> {
        self.provider_pin.verify()?;
        self.image_pin.verify()?;
        if digest(&self.loaded_provider)? != self.provider_pin.sha256
            || digest(&self.loaded_image)? != self.image_pin.sha256
        {
            return Err(invalid(&self.loaded_image, "loaded private image snapshot changed"));
        }
        for attached in &self.attached {
            attached.pin.verify()?;
            if digest(&attached.loaded)? != attached.pin.sha256 {
                return Err(invalid(&attached.loaded, "attached image snapshot changed"));
            }
        }
        Ok(())
    }
    /// Attach a separately admitted image to this exact pinned provider. The
    /// provider is neither copied nor reopened; all images share its lifetime.
    ///
    /// # Safety
    /// The caller owns private source/provider/image admission, including any
    /// automatic initializers. The returned index grants integrity only.
    pub unsafe fn attach(&mut self, image: &Path, sha256: &str) -> Result<usize> {
        self.verify_integrity()?;
        if self.attached.len() >= 255 {
            return Err(invalid(image, "process image count exceeds 256"));
        }
        let pin = PinnedImage::new(image, sha256)?;
        if pin.path.file_name() == self.provider_pin.path.file_name() {
            return Err(invalid(image, "artifact filename conflicts with canonical provider"));
        }
        let snapshot = tempfile::Builder::new()
            .prefix("beskid-attached-image-")
            .tempdir_in(self._snapshot.path())
            .map_err(|error| invalid(image, error))?;
        let loaded = snapshot_image(&pin, snapshot.path())?;
        let library = unsafe { open_image(&loaded, false) }?;
        self.verify_integrity()?;
        pin.verify()?;
        if digest(&loaded)? != pin.sha256 {
            return Err(invalid(&loaded, "attached loaded snapshot changed"));
        }
        self.attached.push(AttachedImage { image: library, pin, loaded, _snapshot: snapshot });
        Ok(self.attached.len())
    }
    /// # Safety
    /// T must be the exact compiler-issued ABI signature of this admitted symbol.
    /// This API cannot qualify a symbol by name or its physical pointer size.
    pub unsafe fn symbol<T>(&self, name: &CStr) -> Result<Symbol<'_, T>> {
        self.verify_integrity()?;
        unsafe { verify_symbol_owner(&self.image, name, &self.loaded_image) }?;
        unsafe { self.image.get(name.to_bytes_with_nul()) }
            .map_err(|error| NativeImageError::Load { path: self.image_pin.path.clone(), message: error.to_string() })
    }
    /// # Safety
    /// T is the exact source-issued ABI for the selected admitted image.
    pub unsafe fn image_symbol<T>(&self, index: usize, name: &CStr) -> Result<Symbol<'_, T>> {
        if index == 0 {
            return unsafe { self.symbol(name) };
        }
        self.verify_integrity()?;
        let attached = self
            .attached
            .get(index - 1)
            .ok_or_else(|| invalid(&self.image_pin.path, "unknown attached image index"))?;
        unsafe { verify_symbol_owner(&attached.image, name, &attached.loaded) }?;
        unsafe { attached.image.get(name.to_bytes_with_nul()) }
            .map_err(|error| NativeImageError::Load { path: attached.pin.path.clone(), message: error.to_string() })
    }
    pub fn image_path_at(&self, index: usize) -> Result<&Path> {
        if index == 0 {
            return Ok(self.image_path());
        }
        self.attached
            .get(index - 1)
            .map(|image| image.pin.path.as_path())
            .ok_or_else(|| invalid(&self.image_pin.path, "unknown attached image index"))
    }
    /// Checks the OS owner of a descriptor returned by an already admitted getter.
    /// Address ownership alone grants no layout, signature or dereference authority.
    pub fn verify_image_address(&self, index: usize, address: *mut std::ffi::c_void) -> Result<()> {
        self.verify_integrity()?;
        let expected = if index == 0 {
            &self.loaded_image
        } else {
            &self
                .attached
                .get(index - 1)
                .ok_or_else(|| invalid(&self.image_pin.path, "unknown attached image index"))?
                .loaded
        };
        unsafe { verify_address_owner(address, expected) }
    }
    /// Provider descriptor address identity, without granting a managed shape.
    pub fn verify_provider_address(&self, address: *mut std::ffi::c_void) -> Result<()> {
        self.verify_integrity()?;
        unsafe { verify_address_owner(address, &self.loaded_provider) }
    }
    pub fn provider_sha256(&self) -> &str {
        &self.provider_pin.sha256
    }
    /// Reject a competing process-visible provider before runtime activation.
    /// This is address/loader identity evidence, not ABI or source admission.
    pub fn verify_process_provider(&self, name: &CStr) -> Result<()> {
        self.verify_integrity()?;
        #[cfg(unix)]
        {
            let process: Library = libloading::os::unix::Library::this().into();
            unsafe { verify_symbol_owner(&process, name, &self.loaded_provider) }
        }
        #[cfg(windows)]
        {
            let _ = name;
            verify_windows_provider_modules(&self.loaded_provider)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = name;
            Err(invalid(&self.loaded_provider, "provider identity inspection unavailable"))
        }
    }
    /// # Safety
    /// T must match the exact admitted canonical provider declaration.
    pub unsafe fn provider_symbol<T>(&self, name: &CStr) -> Result<Symbol<'_, T>> {
        self.verify_integrity()?;
        unsafe { verify_symbol_owner(&self._provider, name, &self.loaded_provider) }?;
        unsafe { self._provider.get(name.to_bytes_with_nul()) }.map_err(|error| NativeImageError::Load {
            path: self.provider_pin.path.clone(),
            message: error.to_string(),
        })
    }
    pub fn provider_path(&self) -> &Path {
        &self.provider_pin.path
    }
    pub fn image_path(&self) -> &Path {
        &self.image_pin.path
    }
}

#[cfg(windows)]
fn verify_windows_provider_modules(expected: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32EnumProcessModules(
            process: *mut std::ffi::c_void,
            modules: *mut *mut std::ffi::c_void,
            bytes: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetModuleFileNameW(module: *mut std::ffi::c_void, buffer: *mut u16, length: u32) -> u32;
    }
    let mut modules = vec![std::ptr::null_mut(); 65536];
    let bytes = (modules.len() * std::mem::size_of::<*mut std::ffi::c_void>()) as u32;
    let mut needed = 0;
    if unsafe { K32EnumProcessModules(GetCurrentProcess(), modules.as_mut_ptr(), bytes, &mut needed) } == 0
        || needed > bytes
        || needed as usize % std::mem::size_of::<*mut std::ffi::c_void>() != 0
    {
        return Err(invalid(expected, "loaded provider module inventory unavailable or exceeds bounds"));
    }
    modules.truncate(needed as usize / std::mem::size_of::<*mut std::ffi::c_void>());
    let expected_name = expected
        .file_name()
        .ok_or_else(|| invalid(expected, "provider filename missing"))?
        .to_string_lossy()
        .to_lowercase();
    let mut matches = 0;
    for module in modules {
        let mut path = vec![0u16; 32768];
        let length = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) };
        if length == 0 || length as usize >= path.len() {
            return Err(invalid(expected, "loaded module path unavailable or exceeds bounds"));
        }
        path.truncate(length as usize);
        let path = PathBuf::from(std::ffi::OsString::from_wide(&path));
        if path.file_name().is_some_and(|name| name.to_string_lossy().to_lowercase() == expected_name) {
            matches += 1;
            if fs::canonicalize(&path).map_err(|error| invalid(expected, error))? != expected {
                return Err(invalid(expected, "competing canonical provider is already loaded"));
            }
        }
    }
    if matches != 1 {
        return Err(invalid(expected, "canonical provider module identity is ambiguous"));
    }
    Ok(())
}
unsafe fn open_image(path: &Path, provider: bool) -> Result<Library> {
    #[cfg(unix)]
    {
        let flags = libloading::os::unix::RTLD_NOW
            | if provider { libloading::os::unix::RTLD_GLOBAL } else { libloading::os::unix::RTLD_LOCAL };
        unsafe { libloading::os::unix::Library::open(Some(path), flags) }
            .map(Into::into)
            .map_err(|error| NativeImageError::Load { path: path.to_owned(), message: error.to_string() })
    }
    #[cfg(windows)]
    {
        let _ = provider;
        // Search dependencies from the exact image directory and system/default
        // DLL directories; never change the process-wide DLL directory.
        unsafe { libloading::os::windows::Library::load_with_flags(path, 0x00000100 | 0x00001000) }
            .map(Into::into)
            .map_err(|error| NativeImageError::Load { path: path.to_owned(), message: error.to_string() })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = provider;
        Err(invalid(path, std::io::Error::new(std::io::ErrorKind::Unsupported, "native image loading unavailable")))
    }
}

// Symbol lookup may otherwise return a dependency or an already loaded image with
// the same loader identity. Check the actual address owner before exposing it.
unsafe fn verify_symbol_owner(library: &Library, name: &CStr, expected: &Path) -> Result<()> {
    let address = unsafe { library.get::<*mut std::ffi::c_void>(name.to_bytes_with_nul()) }
        .map_err(|error| NativeImageError::Load { path: expected.to_owned(), message: error.to_string() })?;
    unsafe { verify_address_owner(*address, expected) }
}
unsafe fn verify_address_owner(address: *mut std::ffi::c_void, expected: &Path) -> Result<()> {
    let actual = unsafe { symbol_image_path(address) }.map_err(|error| invalid(expected, error))?;
    let canonical = fs::canonicalize(actual).map_err(|error| invalid(expected, error))?;
    if canonical != expected {
        return Err(invalid(expected, "native symbol resolved to a foreign image"));
    }
    Ok(())
}
#[cfg(unix)]
unsafe fn symbol_image_path(address: *mut std::ffi::c_void) -> std::io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut info = std::mem::MaybeUninit::<libc::Dl_info>::zeroed();
    if address.is_null() || unsafe { libc::dladdr(address.cast_const(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::other("native symbol has no loaded-image identity"));
    }
    let info = unsafe { info.assume_init() };
    if info.dli_fname.is_null() {
        return Err(std::io::Error::other("native symbol image path is absent"));
    }
    let name = unsafe { CStr::from_ptr(info.dli_fname) };
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
}
#[cfg(windows)]
unsafe fn symbol_image_path(address: *mut std::ffi::c_void) -> std::io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut std::ffi::c_void) -> i32;
        fn GetModuleFileNameW(module: *mut std::ffi::c_void, buffer: *mut u16, length: u32) -> u32;
    }
    let mut module = std::ptr::null_mut();
    if address.is_null() || unsafe { GetModuleHandleExW(0x00000004 | 0x00000002, address.cast(), &mut module) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut buffer = vec![0u16; 32768];
    let count = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    if count == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if count as usize >= buffer.len() {
        return Err(std::io::Error::other("native image path exceeds bound"));
    }
    buffer.truncate(count as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)))
}
#[cfg(not(any(unix, windows)))]
unsafe fn symbol_image_path(_address: *mut std::ffi::c_void) -> std::io::Result<PathBuf> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "symbol image identity unavailable"))
}

fn snapshot_image(pin: &PinnedImage, directory: &Path) -> Result<PathBuf> {
    let name = pin.path.file_name().ok_or_else(|| invalid(&pin.path, "image has no filename"))?;
    let destination = directory.join(name);
    let mut source = fs::File::open(&pin.path).map_err(|error| invalid(&pin.path, error))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| invalid(&destination, error))?;
    let length = source.metadata().map_err(|error| invalid(&pin.path, error))?.len();
    if length > 1024 * 1024 * 1024 {
        return Err(invalid(&pin.path, "image exceeds snapshot bound"));
    }
    let copied = std::io::copy(&mut source.by_ref().take(length + 1), &mut output)
        .map_err(|error| invalid(&destination, error))?;
    if copied != length {
        return Err(invalid(&pin.path, "image length changed while snapshotting"));
    }
    output.sync_all().map_err(|error| invalid(&destination, error))?;
    drop(output);
    if digest(&destination)? != pin.sha256 {
        return Err(invalid(&pin.path, "image changed while snapshotting"));
    }
    let mut permissions = fs::metadata(&destination).map_err(|error| invalid(&destination, error))?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&destination, permissions).map_err(|error| invalid(&destination, error))?;
    pin.verify()?;
    fs::canonicalize(&destination).map_err(|error| invalid(&destination, error))
}
