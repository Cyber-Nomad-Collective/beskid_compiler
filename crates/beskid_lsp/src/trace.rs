//! Timing trace of the server's work, written to stderr when `BESKID_LSP_TRACE` is set.
//!
//! One line per event: `lsp-trace <ms since start> <kind> <fields>`. The soak harness keeps
//! the server's stderr, so a failed soak can be explained from the server's own view of where
//! each message waited (transport, text-sync fence, permits, project database gate) and how
//! long each analysis or diagnostics preparation ran. The check is one atomic load when the
//! trace is off.

use std::sync::OnceLock;
use std::time::Instant;

fn started() -> Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    *STARTED.get_or_init(Instant::now)
}

/// Whether `BESKID_LSP_TRACE` is set (read once).
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let on = std::env::var_os("BESKID_LSP_TRACE").is_some_and(|value| !value.is_empty() && value != "0");
        if on {
            started();
        }
        on
    })
}

/// Milliseconds since the trace started.
pub(crate) fn now_ms() -> u128 {
    started().elapsed().as_millis()
}

/// Write one trace line when the trace is on. `fields` is rendered only then.
pub(crate) fn event(kind: &str, fields: impl FnOnce() -> String) {
    if enabled() {
        eprintln!("lsp-trace {} {kind} {}", now_ms(), fields());
    }
}

/// Measures one section: an event with its duration on drop.
pub(crate) struct Span {
    kind: &'static str,
    label: String,
    started: Instant,
}

impl Span {
    pub(crate) fn start(kind: &'static str, label: impl FnOnce() -> String) -> Option<Self> {
        enabled().then(|| Self { kind, label: label(), started: Instant::now() })
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        eprintln!("lsp-trace {} {} {} took_ms={}", now_ms(), self.kind, self.label, self.started.elapsed().as_millis());
    }
}
