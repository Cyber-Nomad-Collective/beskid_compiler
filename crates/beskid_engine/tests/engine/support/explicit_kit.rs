use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::{Engine, ModHost};

/// Run `entrypoint` of `source` on an engine that loads the debug runtime kit at `prefix`, passed
/// to the engine as a value; no test reads or writes `BESKID_RUNTIME_PREFIX`.
pub fn run_entrypoint_with_kit(
    prefix: &Path,
    source_path: &Path,
    source: &str,
    entrypoint: &str,
    mod_host: &ModHost,
) -> anyhow::Result<String> {
    let target = beskid_engine::host_runtime_target()?;
    let mut engine = Engine::with_runtime_kit(prefix, target, BuildProfile::Debug)?;
    beskid_engine::services::run_entrypoint_with_engine(&mut engine, source_path, source, entrypoint, mod_host)
}
