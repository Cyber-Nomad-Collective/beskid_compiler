//! Linear source-bound canonical runtime host lifetime, shared by native loaders.
//! Admission remains the caller's private producer/provider witness.
use crate::native_image::{NativeImageError, PinnedNativeImages};
use std::path::Path;
use std::{ffi::c_void, marker::PhantomData, rc::Rc, thread::ThreadId};

type HostOpen = unsafe extern "C" fn(*mut *mut c_void) -> i32;
type HostClose = unsafe extern "C" fn(*mut c_void) -> i32;

/// A canonical process lease cannot leave its opening thread. Images remain
/// loaded through heap teardown because managed descriptors belong to them.
pub struct NativeHostLease {
    images: Option<PinnedNativeImages>,
    runtime: *mut c_void,
    close: HostClose,
    thread: ThreadId,
    _thread_bound: PhantomData<Rc<()>>,
}
impl NativeHostLease {
    /// # Safety
    /// The caller must hold source-issued admission proving this exact provider
    /// contains the canonical host_open/host_close contract and image closure.
    /// These hooks use the generated header's runtime layout; this module never
    /// constructs RuntimeState or grants SDK semantic/managed-shape authority.
    pub unsafe fn open(images: PinnedNativeImages) -> Result<Self, NativeImageError> {
        images.verify_process_provider(c"beskid_glue_v1_host_open")?;
        let open = *unsafe { images.provider_symbol::<HostOpen>(c"beskid_glue_v1_host_open") }?;
        let close = *unsafe { images.provider_symbol::<HostClose>(c"beskid_glue_v1_host_close") }?;
        let mut runtime = std::ptr::null_mut();
        let status = unsafe { open(&mut runtime) };
        if status != 0 || runtime.is_null() {
            let path = images.provider_path().to_owned();
            // Malformed open with a nonnull owner cannot be safely torn down.
            // Keep descriptor images alive rather than guessing its lifetime.
            if !runtime.is_null() {
                std::mem::forget(images);
            }
            return Err(NativeImageError::Integrity {
                path,
                message: format!("canonical native host open failed ({status})"),
            });
        }
        Ok(Self {
            images: Some(images),
            runtime,
            close,
            thread: std::thread::current().id(),
            _thread_bound: PhantomData,
        })
    }
    pub fn images(&self) -> &PinnedNativeImages {
        self.images.as_ref().expect("live native host lease")
    }
    /// Attach another producer-admitted artifact without opening a second
    /// runtime or provider. Its descriptors remain pinned through heap teardown.
    ///
    /// # Safety
    /// Caller must privately prove current source/image admission and the exact
    /// canonical provider digest; hashes alone cannot grant semantic authority.
    pub unsafe fn attach_image(
        &mut self,
        image: &Path,
        image_sha256: &str,
        provider_sha256: &str,
    ) -> Result<usize, NativeImageError> {
        if self.thread != std::thread::current().id() || self.runtime.is_null() {
            return Err(NativeImageError::Integrity {
                path: image.to_owned(),
                message: "native host attachment is outside its opening thread/lifetime".into(),
            });
        }
        let images = self.images.as_mut().expect("live host lease");
        if images.provider_sha256() != provider_sha256 {
            return Err(NativeImageError::Integrity {
                path: image.to_owned(),
                message: "artifact requires a different canonical provider".into(),
            });
        }
        images.verify_process_provider(c"beskid_glue_v1_host_open")?;
        let index = unsafe { images.attach(image, image_sha256) }?;
        images.verify_process_provider(c"beskid_glue_v1_host_open")?;
        Ok(index)
    }
    /// Complete heap teardown before releasing either loader handle.
    pub fn close(mut self) -> Result<(), NativeImageError> {
        self.close_inner()
    }
    fn close_inner(&mut self) -> Result<(), NativeImageError> {
        let Some(images) = self.images.take() else { return Ok(()) };
        if self.thread != std::thread::current().id() {
            let path = images.provider_path().to_owned();
            std::mem::forget(images);
            return Err(NativeImageError::Integrity { path, message: "native host lease changed threads".into() });
        }
        let status = unsafe { (self.close)(self.runtime) };
        self.runtime = std::ptr::null_mut();
        if status != 0 {
            let path = images.provider_path().to_owned();
            // A failed shutdown may retain live GC descriptor references. Never
            // unload their owner images on an uncertain teardown boundary.
            std::mem::forget(images);
            return Err(NativeImageError::Integrity {
                path,
                message: format!("canonical native host close failed ({status}); images retained"),
            });
        }
        drop(images);
        Ok(())
    }
}
impl Drop for NativeHostLease {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}
