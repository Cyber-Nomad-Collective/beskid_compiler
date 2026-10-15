//! Shared grammar/compiler revision baked into unit artifact fingerprints.

/// Package version plus `BESKID_GRAMMAR_DIGEST`, the build-time SHA-256 (64-bit prefix) of every
/// `beskid_analysis` source that decides how text becomes a syntax tree: `beskid.pest`,
/// `beskid_doc.pest`, `parser.rs`, `doc_comment_parser.rs`, `parsing/**`, `syntax/**`,
/// `macros/**`, `services/parse.rs` and `services/parse_recovery/**` (CR-normalized; see
/// `build.rs`). Any change to them invalidates persisted syntax artifacts and Salsa snapshots.
pub const GRAMMAR_REVISION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("BESKID_GRAMMAR_DIGEST"));
