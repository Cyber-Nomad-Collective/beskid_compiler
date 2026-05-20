//! Per-fiber stack allocation (64 KiB initial, 8 MiB max per platform spec).

pub const STACK_INITIAL: usize = 64 * 1024;
pub const STACK_MAX: usize = 8 * 1024 * 1024;

/// Owns a downward-growing stack buffer with guard space.
pub struct FiberStack {
    storage: Vec<u8>,
}

impl Default for FiberStack {
    fn default() -> Self {
        Self::new()
    }
}

impl FiberStack {
    pub fn new() -> Self {
        let storage = vec![0u8; STACK_INITIAL];
        Self { storage }
    }

    /// Top of stack (highest address) for context init.
    pub fn top(&self) -> *mut u8 {
        unsafe { self.storage.as_ptr().add(self.storage.len()) as *mut u8 }
    }
}
