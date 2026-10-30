//! Staged-evidence tests: proofs about the runtime kit that the staging scripts publish, not about
//! a kit a test can build for itself.
//!
//! Such a test is `#[ignore = "staged evidence: ..."]`, so a local `cargo test` lists it as ignored
//! instead of passing it silently, and its first line calls [`staged_kit`]. CI and the staging
//! scripts run it with `--include-ignored` (or `--ignored --exact`) in the environment that
//! `scripts/stage-native-runtime-kit.sh` prepares. A test that only needs *a* kit uses the shared
//! kit of `beskid_tests_mod_host` instead and is not ignored.

use std::path::PathBuf;

/// Names the staged runtime-kit installation prefix.
pub const RUNTIME_PREFIX_ENV: &str = "BESKID_RUNTIME_PREFIX";
/// Names the staged profile: `debug` or `release`, with no default.
pub const RUNTIME_KIT_PROFILE_ENV: &str = "BESKID_RUNTIME_KIT_PROFILE";

/// Profile of the staged kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagedProfile {
    Debug,
    Release,
}

/// The staged runtime kit named by the evidence environment.
#[derive(Clone, Debug)]
pub struct StagedKit {
    /// Installation prefix (`<prefix>/lib/beskid-runtime/...`).
    pub prefix: PathBuf,
    pub profile: StagedProfile,
}

impl StagedKit {
    /// The profile directory name (`debug` or `release`).
    pub fn profile_name(&self) -> &'static str {
        match self.profile {
            StagedProfile::Debug => "debug",
            StagedProfile::Release => "release",
        }
    }
}

const HOW_TO_SET: &str = "run the staged-evidence tests with `--include-ignored` in the environment of \
                          `scripts/stage-native-runtime-kit.sh` (the compiler Rust gate runtime phase sets \
                          both variables)";

/// The staged kit for a staged-evidence test. Panics, naming the variable and how to set it, when
/// [`RUNTIME_PREFIX_ENV`] is unset, empty or not a directory, or when [`RUNTIME_KIT_PROFILE_ENV`]
/// is not exactly `debug` or `release`. It never falls back to a default.
pub fn staged_kit() -> StagedKit {
    let prefix = match std::env::var_os(RUNTIME_PREFIX_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => panic!("staged evidence needs {RUNTIME_PREFIX_ENV} to name the staged runtime-kit prefix; {HOW_TO_SET}"),
    };
    assert!(
        prefix.is_dir(),
        "staged evidence: {RUNTIME_PREFIX_ENV}={} is not a directory; {HOW_TO_SET}",
        prefix.display()
    );
    let profile = match std::env::var(RUNTIME_KIT_PROFILE_ENV).as_deref() {
        Ok("debug") => StagedProfile::Debug,
        Ok("release") => StagedProfile::Release,
        Ok(other) => panic!("staged evidence: {RUNTIME_KIT_PROFILE_ENV}=`{other}` must be `debug` or `release`"),
        Err(_) => panic!("staged evidence needs {RUNTIME_KIT_PROFILE_ENV} (`debug` or `release`); {HOW_TO_SET}"),
    };
    StagedKit { prefix, profile }
}
