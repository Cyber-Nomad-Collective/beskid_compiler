//! Stability-proof harnesses.
//!
//! * [`mutate`] is the deterministic mutation generator behind the internal-error sweep
//!   (`scripts/proof/ice_sweep.py`).
//! * [`framing`] and [`soak`] are the scripted language-server client that drives `beskid_lsp`
//!   over stdio (`beskid-soak`).
//!
//! The crate has no dependency on the compiler crates on purpose: it exercises the installed
//! binaries from the outside, as a user or an editor would.

pub mod framing;
pub mod mutate;
pub mod rng;
pub mod soak;
