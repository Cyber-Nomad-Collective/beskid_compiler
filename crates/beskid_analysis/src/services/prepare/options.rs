//! Prepare options and the prepared compilation result.

use std::sync::Arc;

use anyhow::Result;

use crate::projects::ProgramAssembly;
use crate::syntax::Spanned;

use super::super::front_end::{ExecutableFrontEnd, FrontEndOptions};

/// Options for the prepare spine.
///
/// There is no default: a caller chooses its native Mod host with [`Self::without_mods`] (the plan
/// must be Mod-free) or [`Self::with_mod_host`].
#[derive(Clone)]
pub struct PrepareOptions {
    /// The native Mod host that loads and runs the plan's Mods; borrowed semantic authority remains
    /// invocation-scoped.
    pub mod_host: crate::mod_host::ModHost,
    pub front_end: FrontEndOptions,
    /// Native Mod builds only: assemble the compiler-owned SDK adapter sources with the Mod
    /// (see `AssemblyOptions::native_mod_adapter_sources`).
    pub native_mod_adapter_sources: bool,
}

impl PrepareOptions {
    /// Options for a Mod-free plan: preparing a plan with a native Mod dependency fails before
    /// `mod.load` with an error that names each Mod and how to provide a host.
    pub fn without_mods() -> Self {
        Self::with_mod_host(crate::mod_host::ModHost::NoMods)
    }

    /// Options that run the plan's native Mods in `mod_host`.
    pub fn with_mod_host(mod_host: crate::mod_host::ModHost) -> Self {
        Self { mod_host, front_end: FrontEndOptions::default(), native_mod_adapter_sources: false }
    }
}

/// Result of the unified prepare spine.
///
/// `executable` exists whenever the spine completed: the query gate is the one typing authority,
/// so nothing after it can reject the program before lowering. A gate error is reported through
/// the collected diagnostics (diagnostics path) or fails preparation (executable path).
pub struct PreparedCompilation {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
    pub executable: Arc<ExecutableFrontEnd>,
}

impl PreparedCompilation {
    /// The executable front end for codegen, taken out of the entry session cache.
    pub fn into_executable(self) -> Result<ExecutableFrontEnd> {
        Arc::try_unwrap(self.executable).map_err(|shared| {
            anyhow::anyhow!(
                "executable front-end is still shared in the entry session cache (strong_refs={})",
                Arc::strong_count(&shared)
            )
        })
    }

    pub fn executable(&self) -> &ExecutableFrontEnd {
        &self.executable
    }

    /// Syntax-only project assembly for generation-safe consumers (LSP, queries, ISLE): the
    /// post-mod-rewrite entry program projected over the immutable source units.
    pub fn syntax_assembly(&self) -> crate::projects::ProgramAssembly {
        self.executable.syntax_assembly()
    }
}

impl std::fmt::Debug for PrepareOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrepareOptions")
            .field("front_end", &self.front_end)
            .field("mod_host", &self.mod_host)
            .field("native_mod_adapter_sources", &self.native_mod_adapter_sources)
            .finish()
    }
}
