//! `ExternalLibrary` provider trait and closed registry for the
//! [`beskid import lib`](https://beskid-lang.org/platform-spec/tooling/foreign-library-import/cli-import-lib-command/)
//! v0.3 platform-spec feature.
//!
//! See `site/website/src/content/docs/platform-spec/tooling/foreign-library-import/` for the
//! normative contract and the closed-registry ADR (`D-TOOL-FLI-0002`).

pub mod error;
pub mod manifest_merge;
pub mod providers;
pub mod registry;
pub mod resolution;
pub mod trait_def;

pub use error::LibraryResolveError;
pub use manifest_merge::{
    LinkMergeOutcome, merge_resolution_into_manifest_source, render_manifest_with_link,
};
pub use providers::{CPosixProvider, PosixProvider};
pub use registry::{
    ExternalLibraryRegistry, current_host_key, default_registry, known_provider_ids,
};
pub use resolution::LibraryResolution;
pub use trait_def::ExternalLibrary;
