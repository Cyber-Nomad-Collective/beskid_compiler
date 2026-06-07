//! HTTP client, models, and `beskid pckg` CLI for the pckg package registry.

pub mod api_doc;
pub mod api_keys;
pub mod cli;
pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod pack;
pub mod packages;
pub mod progress;
pub mod users;

pub use cli::PckgArgs;
pub use client::PckgClient;
pub use config::PckgClientConfig;
pub use error::PckgError;
pub use pack::{
    PACKAGE_KIND_LIBRARY, PACKAGE_KIND_TEMPLATE, PACKAGE_KIND_TOOL, PackProfile,
    PackProfileOverride, TEMPLATE_JSON_REL, TemplatePackageSummary, apply_pack_readme,
    build_package_json, collect_pack_entries, detect_pack_profile,
    detect_pack_profile_with_override, load_template_package_summary, normalize_rel_path,
    strip_template_pack_excludes, strip_tool_pack_excludes, template_summary_json,
    zip_to_pckg_error,
};
