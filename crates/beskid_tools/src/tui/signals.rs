//! Cross-thread redraw invalidation for the tuirealm shell runtime.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cloneable signal for requesting a redraw from any thread.
#[derive(Debug, Clone, Default)]
pub struct RedrawSignal {
    dirty: Arc<AtomicBool>,
}

impl RedrawSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_redraw(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    pub fn is_redraw_requested(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    pub fn take_redraw_request(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::RedrawSignal;

    #[test]
    fn redraw_signal_round_trip() {
        let signal = RedrawSignal::new();
        assert!(!signal.take_redraw_request());
        signal.request_redraw();
        assert!(signal.take_redraw_request());
    }
}
