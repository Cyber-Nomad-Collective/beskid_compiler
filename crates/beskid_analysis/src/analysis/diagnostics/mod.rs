//! The one structured diagnostic, [`SemanticDiagnostic`], and helpers for codes, spans and
//! severity.
//!
//! A diagnostic names one cause: a one-sentence `message`, a primary span anchored where the fix
//! goes with its `label`, labelled [`RelatedLocation`]s for the other places involved, optional
//! `help`, [`Fix`]es with [`Applicability`], and [`DiagnosticTag`]s. Renderers read these parts:
//! the CLI draws them with miette, `--error-format=json` serialises them ([`json`]), and the
//! language server maps them to LSP diagnostics. Before rendering, spans in materialized copies
//! are mapped back to the authored source ([`source_map`]) and duplicates are dropped
//! ([`dedupe`]).

pub mod dedupe;
pub mod json;
mod miette;
pub mod model;
mod semantic;
pub mod source_map;

pub use self::miette::MietteReportError;
pub use dedupe::dedupe_diagnostics;
pub use model::{Applicability, DiagnosticTag, Fix, FixEdit, RelatedLocation, RelatedSource, line_column};
pub use semantic::{SemanticDiagnostic, Severity, make_diagnostic, span_to_sourcespan, span_to_sourcespan_within};
