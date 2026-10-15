//! Native Mod host for lib tests that prepare Corelib-dependent projects.
//!
//! Corelib depends on the native Serialization Mod, so preparing a Corelib project runs that Mod.
//! The server never builds Mods, and a test executable has no installed `beskid` CLI next to it,
//! so these tests select the workspace CLI and the shared debug runtime kit as the session's Mod
//! host (the `modHost` initialization option with `path` and `runtimePrefix` in production) after
//! provisioning the project's Mods with them. Nothing touches the process environment.

use std::path::Path;

use beskid_tests_mod_host::{ModHostScope, provisioned_cli};
use beskid_tools::native_mods::ModHostSelection;

use crate::session::store::State;

/// A provisioned Mod host for one test: the workspace CLI and the shared runtime kit, as values.
pub(crate) struct ProvisionedModHost {
    mod_host: ModHostSelection,
}

impl ProvisionedModHost {
    /// Provision every native Mod of the project that owns `source_path` and select the
    /// provisioning CLI and runtime kit as the Mod host.
    pub(crate) fn for_source(source_path: &Path) -> Self {
        let scope = ModHostScope::shared().expect("stage the shared runtime kit");
        let context = beskid_analysis::CompilationContext::try_for_analysis_path(source_path, None)
            .unwrap_or_else(|| panic!("compilation context for {}", source_path.display()));
        let plan = context.compile_plan.as_ref().expect("project-backed compile plan");
        scope.corelib_mod_host(plan).expect("provision the project's native Mods");
        let cli = provisioned_cli().expect("workspace Mod host CLI");
        let mod_host = ModHostSelection::explicit(&cli)
            .expect("valid Mod host executable")
            .with_runtime_kit(scope.runtime().clone());
        Self { mod_host }
    }

    /// A fresh session whose Mod host is the provisioned CLI and runtime kit.
    pub(crate) fn state(&self) -> State {
        State { mod_host: self.mod_host.clone(), ..State::default() }
    }
}
