//! Shared `--runtime-profile` flag for AOT/JIT commands.

use clap::ValueEnum;
use beskid_aot::RuntimeLinkProfile;

/// Which runtime/host artifacts to link at build and startup time.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum CliRuntimeProfile {
    /// Language runtime only; host dispatch tags trap unless registered elsewhere.
    Minimal,
    /// Language runtime plus `beskid_host` (default).
    #[default]
    Std,
}

impl From<CliRuntimeProfile> for RuntimeLinkProfile {
    fn from(value: CliRuntimeProfile) -> Self {
        match value {
            CliRuntimeProfile::Minimal => RuntimeLinkProfile::Minimal,
            CliRuntimeProfile::Std => RuntimeLinkProfile::Std,
        }
    }
}
