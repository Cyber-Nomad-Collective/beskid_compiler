//! Unified compilation prepare spine consumed by analyze, run, build, test, and LSP.

mod diagnostics;
mod entry_points;
mod options;
mod spine;

pub use diagnostics::resolved_input_from_plan;
use diagnostics::{
    collect_analyzer_diagnostics, collect_analyzer_fixes, dedupe_diagnostics, semantic_facts_errors_to_diagnostics,
    typed_fingerprint, typed_fingerprint_types,
};
use entry_points::session_fingerprint_field;
pub use entry_points::{
    TryDiagnosticAuthority, prepare_compilation, prepare_compilation_diagnostics,
    prepare_compilation_diagnostics_isolated, prepare_compilation_with_try_authority,
};
pub use options::{PrepareOptions, PreparedCompilation};
use spine::run_prepare_spine;

#[cfg(test)]
mod tests;
