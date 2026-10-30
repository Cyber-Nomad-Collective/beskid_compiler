//! Release host harness support.
//!
//! The release case kind `installed_host_harness` runs the tests of this crate against the
//! installed candidate prefix named by `BESKID_QUALIFIED_PREFIX`. The policy
//! `installed_prefix_only` is enforced here, not by review: [`installed::Cli`] is the only way a
//! test may run a compiler, it panics when the prefix is absent (never skips), it refuses an
//! environment that carries developer overrides, and every invocation writes one proof line to the
//! process stderr that the producer and the release reader re-parse:
//!
//! ```text
//! beskid-installed-invocation sha256=<digest of the executed CLI bytes> argv0=<flat executable name>
//! ```
//!
//! The Glue cases (`tests/glue_owned.rs`) keep their own installation checks: they admit the
//! driver and runtime kit of a private copy of the same prefix and fail when either is absent.
//!
//! Nothing here derives a path from the source checkout (no compile-time manifest-directory paths).
//! Fixtures are embedded in the test sources or built in temporary directories.

pub mod installed;
pub mod project;
pub mod registry;
