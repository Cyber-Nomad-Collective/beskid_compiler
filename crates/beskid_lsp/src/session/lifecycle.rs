mod diagnostics;
mod documents;
mod facts;
mod revisions_resolution;
mod typed_prepare;

pub use diagnostics::publish_diagnostics_for_uri;
pub use documents::{
    build_document, rebuild_open_document_syntax_facts, remove_document, set_disk_snapshot, set_document,
};
pub use typed_prepare::schedule_typed_prepare_rebuild;

#[cfg(test)]
mod tests;
