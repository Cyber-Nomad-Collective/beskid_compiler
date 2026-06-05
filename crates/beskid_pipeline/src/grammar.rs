//! Shared grammar/compiler revision baked into unit artifact fingerprints.

/// Bump when parse, macro expansion, or HIR lowering semantics change.
pub const GRAMMAR_REVISION: &str = env!("CARGO_PKG_VERSION");
