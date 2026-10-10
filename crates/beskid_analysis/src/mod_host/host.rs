//! The native Mod host a preparation runs Mods in, and the exact runtime kit its artifacts load
//! against.
//!
//! There is no implicit host. A caller that prepares a plan chooses [`ModHost::NoMods`] (the plan
//! must be Mod-free) or [`ModHost::native`] with a qualified invoker and the
//! [`RuntimeKitSelection`] that invoker's artifacts were qualified against. Loading and
//! qualification read the runtime kit only from that value, never from the process environment.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use beskid_abi::runtime_kit::BuildProfile;

use super::invoker::ContractInvoker;
use super::types::DiscoveredMod;

/// One exact ABI-v5 runtime kit: an installation prefix and a build profile.
///
/// Native Mod descriptors validate against this kit, cache entries of other profiles are not
/// candidates, and producer-side qualification binds the artifact to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeKitSelection {
    prefix: PathBuf,
    profile: BuildProfile,
}

impl RuntimeKitSelection {
    /// The kit this process resolves for itself: `BESKID_RUNTIME_PREFIX`, else the installation
    /// prefix of the running executable, with `BESKID_RUNTIME_KIT_PROFILE` (default `debug`).
    pub fn installed() -> Result<Self> {
        let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()
            .map_err(|error| anyhow::anyhow!("native Mod runtime prefix: {error}"))?;
        let profile = super::descriptor::native_mod_runtime_profile()?;
        Ok(Self { prefix, profile })
    }

    /// An explicit kit. `prefix` must be an absolute path to an installation prefix that holds an
    /// exact runtime kit for the native host target and `profile`; the prefix is canonicalized.
    pub fn explicit(prefix: &Path, profile: BuildProfile) -> Result<Self> {
        if !prefix.is_absolute() {
            bail!("native Mod runtime prefix {} is not an absolute path", prefix.display());
        }
        let canonical =
            prefix.canonicalize().with_context(|| format!("resolve native Mod runtime prefix {}", prefix.display()))?;
        if !canonical.is_dir() {
            bail!("native Mod runtime prefix {} is not a directory", canonical.display());
        }
        let target = beskid_abi::runtime_kit::host_runtime_target()
            .map_err(|error| anyhow::anyhow!("native Mod host target: {error}"))?;
        beskid_abi::runtime_kit::resolve_installed_runtime_kit(&canonical, &target, profile).map_err(|error| {
            anyhow::anyhow!(
                "native Mod runtime prefix {} holds no exact {profile:?} runtime kit for {}: {error:?}",
                canonical.display(),
                target.triple.as_str()
            )
        })?;
        Ok(Self { prefix: canonical, profile })
    }

    /// The installation prefix of the kit.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    /// The build profile of the kit.
    pub fn profile(&self) -> BuildProfile {
        self.profile
    }

    /// Unit-test fixture for orchestration tests that never load a native kit.
    #[cfg(test)]
    pub(crate) fn fixture(prefix: &Path, profile: BuildProfile) -> Self {
        Self { prefix: prefix.to_path_buf(), profile }
    }
}

/// How a preparation runs native Mods. A caller always chooses one; there is no default.
#[derive(Clone)]
pub enum ModHost {
    /// The caller prepares Mod-free plans only. A plan with a native Mod dependency fails before
    /// `mod.load` with an error that names every Mod and how to provide a host.
    NoMods,
    /// Mods dispatch through `invoker`, and their artifacts load and validate against `runtime`,
    /// the kit the invoker's artifacts were qualified against.
    Native { invoker: Arc<dyn ContractInvoker>, runtime: RuntimeKitSelection },
}

impl ModHost {
    /// A native Mod host.
    pub fn native(invoker: Arc<dyn ContractInvoker>, runtime: RuntimeKitSelection) -> Self {
        Self::Native { invoker, runtime }
    }

    /// The dispatching invoker, when this is a native host.
    pub fn invoker(&self) -> Option<&dyn ContractInvoker> {
        match self {
            Self::NoMods => None,
            Self::Native { invoker, .. } => Some(invoker.as_ref()),
        }
    }

    /// The runtime kit, when this is a native host.
    pub fn runtime(&self) -> Option<&RuntimeKitSelection> {
        match self {
            Self::NoMods => None,
            Self::Native { runtime, .. } => Some(runtime),
        }
    }

    /// Whether this host runs native Mods.
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    /// The invoker and runtime kit for `mods`, or the precise reason this host cannot run them.
    pub(crate) fn require<'a>(
        &self,
        mods: impl IntoIterator<Item = &'a DiscoveredMod>,
    ) -> Result<(&dyn ContractInvoker, &RuntimeKitSelection)> {
        match self {
            Self::Native { invoker, runtime } => Ok((invoker.as_ref(), runtime)),
            Self::NoMods => Err(anyhow::anyhow!("{}", no_mod_host_message(mods))),
        }
    }
}

impl std::fmt::Debug for ModHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMods => formatter.write_str("ModHost::NoMods"),
            Self::Native { runtime, .. } => formatter.debug_struct("ModHost::Native").field("runtime", runtime).finish(),
        }
    }
}

fn no_mod_host_message<'a>(mods: impl IntoIterator<Item = &'a DiscoveredMod>) -> String {
    let named = mods
        .into_iter()
        .map(|discovered| {
            format!("`{}` ({})", discovered.dependency_name, discovered.manifest_path.display())
        })
        .collect::<Vec<_>>();
    format!(
        "native Mod dependency {} requires a native Mod host, but this preparation runs with `ModHost::NoMods`; \
         an in-process caller (engine, JIT, test harness) passes `ModHost::native(invoker, runtime)` with an invoker \
         qualified against that runtime kit (for example from \
         `beskid_tools::native_mods::ModHostSelection::mod_host_for_plan`), or builds and runs the project with the \
         `beskid` CLI, which is its own Mod host",
        named.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered(name: &str) -> DiscoveredMod {
        DiscoveredMod {
            dependency_name: name.to_owned(),
            project_name: name.to_owned(),
            project_root: PathBuf::from("/mods").join(name),
            manifest_path: PathBuf::from("/mods").join(name).join(format!("{name}.bproj")),
            source_root: PathBuf::from("/mods").join(name).join("Src"),
            mod_section: None,
        }
    }

    #[test]
    fn no_mods_names_every_mod_and_the_remedy() {
        let mods = [discovered("Serialization"), discovered("Glue")];
        let error = ModHost::NoMods.require(mods.iter()).err().expect("NoMods cannot run a Mod");
        let message = error.to_string();
        assert!(message.contains("`Serialization`") && message.contains("`Glue`"), "{message}");
        assert!(message.contains("Serialization.bproj"), "{message}");
        assert!(message.contains("ModHost::native"), "{message}");
        assert!(message.contains("`beskid` CLI"), "{message}");
    }

    #[test]
    fn native_host_yields_its_invoker_and_kit() {
        let runtime = RuntimeKitSelection::fixture(Path::new("/kit"), BuildProfile::Release);
        let host = ModHost::native(Arc::new(super::super::invoker::StubContractInvoker::new()), runtime.clone());
        let (_, selected) = host.require([discovered("ModA")].iter()).expect("native host");
        assert_eq!(selected, &runtime);
        assert!(host.is_native());
        assert!(!ModHost::NoMods.is_native());
    }

    #[test]
    fn explicit_kit_rejects_relative_missing_and_kitless_prefixes() {
        for prefix in [Path::new("relative/prefix"), Path::new("/nonexistent/beskid-runtime-prefix")] {
            assert!(RuntimeKitSelection::explicit(prefix, BuildProfile::Debug).is_err(), "{}", prefix.display());
        }
        let empty = tempfile::tempdir().expect("empty prefix");
        let error = RuntimeKitSelection::explicit(empty.path(), BuildProfile::Debug).expect_err("no kit");
        assert!(error.to_string().contains("holds no exact"), "{error}");
    }
}
