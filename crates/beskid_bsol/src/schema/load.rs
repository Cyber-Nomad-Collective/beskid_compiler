//! Embedded profile registration mirror for `beskid_bsol`.
//!
//! Canonical loader: `beskid_bsol/crates/bsol-schema/src/load.rs`.
//! `shell.pages.v1` is included in `EMBEDDED_PROFILES` alongside board/project profiles.

pub const SHELL_PAGES_V1: &str = include_str!("../../../schemas/shell.pages.v1.bsol");
