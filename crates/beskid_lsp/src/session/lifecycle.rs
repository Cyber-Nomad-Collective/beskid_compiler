mod diagnostics;
mod documents;
mod facts;
mod persistence;
mod revisions_resolution;

pub use diagnostics::publish_diagnostics_for_uri;
pub use documents::{
    build_document, rebuild_open_document_syntax_facts, remove_document, set_disk_snapshot, set_document,
};
pub(crate) use facts::bsol_semantic_token_candidates;
pub(crate) use persistence::DEFAULT_PERSISTENCE_DEBOUNCE;
pub use persistence::{
    apply_persistence_config, persistence_config_from_configuration, persistence_config_from_value, save_snapshot_now,
    schedule_persistence_snapshot_save,
};

#[cfg(test)]
mod tests;
