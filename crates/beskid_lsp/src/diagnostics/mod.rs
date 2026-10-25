//! Map Beskid parse/semantic/manifest errors to generation-bound syntax diagnostic facts
//! and LSP [`Diagnostic`] values.
//!
//! Production publish/refresh paths consume [`SyntaxDiagnostic`] facts attached to the
//! current buffer revision. They never read `Document.analysis` or HIR snapshots.
//!
//! Editor shape: one diagnostic per cause at its exact span, its message
//! the cause sentence plus at most one `Help:` line, `code` with a `codeDescription` link to the
//! registry page, `source` `beskid`, labelled `relatedInformation`, tags, and its fixes in `data`
//! ([`quick_fixes_from_data`] turns them into quick fixes). Each related place in the buffer also
//! gets a Hint marker with the same code, the label as message and a link back to the cause. The
//! compiler has already mapped spans to the authored source; the editor does no mapping.

mod manifest;
mod preparation;
mod to_lsp;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use preparation::analyze_document;
pub use preparation::{ProjectPreparation, collect_syntax_diagnostics};
pub(crate) use preparation::{
    EditorPreparePass, prepare_project_diagnostics_from_assembled, prepare_project_syntax_facts,
    project_preparation_failure,
};
pub(crate) use to_lsp::quick_fixes_from_data;
#[cfg(test)]
pub(crate) use to_lsp::FixFact;
pub use to_lsp::{DiagnosticDetail, lsp_diagnostics_from_syntax};
