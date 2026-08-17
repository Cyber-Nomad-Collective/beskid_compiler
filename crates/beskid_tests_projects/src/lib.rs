//! Project, workspace, spine, and template integration tests.
//!
//! Run with: `cargo test -p beskid_tests_projects` (append `--features slow` for slow gates).

#[cfg(test)]
pub mod projects;

#[cfg(test)]
pub mod spine;

#[cfg(test)]
mod template;
