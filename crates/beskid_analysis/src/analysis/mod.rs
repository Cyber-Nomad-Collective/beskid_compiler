//! Pluggable semantic [`rules::Rule`] pipeline, [`diagnostics::SemanticDiagnostic`] emission, and issue kinds.

pub mod builtin;
pub mod diagnostic_kinds;
pub mod diagnostics;
pub mod rules;

pub use builtin::builtin_rules;
pub use diagnostic_kinds::SemanticIssueKind;
pub use diagnostics::{MietteReportError, SemanticDiagnostic, Severity, span_to_sourcespan};
pub use rules::{AnalysisOptions, AnalysisResult, Rule, RuleContext, run_rules};
