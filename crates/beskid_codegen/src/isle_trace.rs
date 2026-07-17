//! Opt-in, stderr-based tracing for the syntax → ISLE → CLIF boundary.
//!
//! This deliberately does not rely on a global `tracing` subscriber: Corelib CI invokes the
//! compiler through several binaries, while this switch must always leave a readable failure
//! trail when explicitly requested.

use std::sync::OnceLock;

/// Enables detailed syntax/ISLE emission records when set to `1` or `true`.
pub(crate) const ENV: &str = "BESKID_COMPILER_TRACE";

static ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var(ENV).is_ok_and(|value| enabled_for(&value)))
}

fn enabled_for(value: &str) -> bool {
    matches!(value, "1" | "true" | "TRUE" | "True")
}

/// Emit one stable, grep-friendly trace record without allocating when tracing is disabled.
pub(crate) fn event(build: impl FnOnce() -> String) {
    if enabled() {
        eprintln!("beskid-isle-trace {}", build());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepts_only_the_explicit_trace_flag() {
        assert!(super::enabled_for("1"));
        assert!(super::enabled_for("true"));
        assert!(!super::enabled_for("0"));
        assert!(!super::enabled_for("yes"));
    }
}
