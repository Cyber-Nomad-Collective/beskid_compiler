//! Cooperative interrupt flag shared between the TUI thread and CLI main thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct InterruptFlag(Arc<AtomicBool>);

impl InterruptFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Default for InterruptFlag {
    fn default() -> Self {
        Self::new()
    }
}
