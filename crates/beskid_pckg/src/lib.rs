//! HTTP client, models, and `beskid pckg` CLI for the pckg package registry.

pub mod api_doc;
pub mod api_keys;
pub mod cli;
pub mod pack;
pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod packages;
pub mod users;

pub use cli::PckgArgs;
pub use pack::{apply_pack_readme, collect_pack_entries, normalize_rel_path, zip_to_pckg_error};
pub use client::PckgClient;
pub use config::PckgClientConfig;
pub use error::PckgError;
