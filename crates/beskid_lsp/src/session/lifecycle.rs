mod diagnostics;
mod documents;
mod facts;
mod revisions_resolution;

pub use diagnostics::{publish_analyzed_diagnostics, publish_diagnostics_for_uri};
#[cfg(test)]
pub use documents::set_document;
pub use documents::{
    analyze_open_document, build_document, rebuild_open_document_syntax_facts, remove_document, set_disk_snapshot,
};
pub(crate) use documents::{build_initial_workspace_document, index_project_documents, superseded};
pub(crate) use facts::{bsol_semantic_token_candidates, syntax_symbols_for_program};

#[cfg(test)]
mod tests;
