//! External root tracking for embedding runtimes that keep raw pointers.

use parking_lot::Mutex;

#[derive(Default)]
pub struct ExternalRootSet {
    registered_roots: Mutex<Vec<*mut *mut u8>>,
    handles: Mutex<Vec<*mut u8>>,
}

// Raw pointer slots are managed by embedder protocol and guarded by mutexes.
unsafe impl Send for ExternalRootSet {}
unsafe impl Sync for ExternalRootSet {}

impl ExternalRootSet {
    pub fn register_root(&self, ptr_addr: *mut *mut u8) {
        if ptr_addr.is_null() {
            return;
        }
        self.registered_roots.lock().push(ptr_addr);
    }

    pub fn unregister_root(&self, ptr_addr: *mut *mut u8) {
        if ptr_addr.is_null() {
            return;
        }
        self.registered_roots
            .lock()
            .retain(|entry| *entry != ptr_addr);
    }

    pub fn push_handle(&self, ptr: *mut u8) -> u64 {
        let mut handles = self.handles.lock();
        let idx = handles.len();
        handles.push(ptr);
        idx as u64
    }

    pub fn drop_handle(&self, handle: u64) {
        if let Some(slot) = self.handles.lock().get_mut(handle as usize) {
            *slot = std::ptr::null_mut();
        }
    }

    pub(crate) fn snapshot_roots(&self) -> Vec<*mut u8> {
        let mut out = Vec::new();

        let registered = self.registered_roots.lock();
        for ptr_addr in registered.iter().copied() {
            if ptr_addr.is_null() {
                continue;
            }
            // SAFETY: embedder-provided root slots are assumed valid while registered.
            let ptr = unsafe { *ptr_addr };
            if !ptr.is_null() {
                out.push(ptr);
            }
        }
        drop(registered);

        let handles = self.handles.lock();
        for ptr in handles.iter().copied() {
            if !ptr.is_null() {
                out.push(ptr);
            }
        }

        out
    }

    pub fn root_count(&self) -> usize {
        let registered = self.registered_roots.lock().len();
        let handles = self
            .handles
            .lock()
            .iter()
            .filter(|ptr| !ptr.is_null())
            .count();
        registered + handles
    }
}
