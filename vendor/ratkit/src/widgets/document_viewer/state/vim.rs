//! Minimal Vim navigation state shared by document viewers.

/// Tracks multi-key Vim navigation prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VimState {
    /// Whether a `g` prefix is waiting for a second key.
    pub pending_g: bool,
}

impl VimState {
    /// Clears pending Vim prefixes.
    pub fn clear(&mut self) {
        self.pending_g = false;
    }
}
