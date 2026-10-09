//! Native Mod host for lib tests that prepare Corelib-dependent projects.
//!
//! Corelib depends on the native Serialization Mod, so preparing a Corelib project runs that Mod.
//! The server never builds Mods, and a test executable has no installed `beskid` CLI next to it,
//! so these tests select the workspace CLI as the session's Mod host (the `modHost`
//! initialization option in production) after provisioning the project's Mods with it.

use std::path::Path;

use beskid_tests_mod_host::{ModHostScope, provisioned_cli};
use beskid_tools::native_mods::ModHostSelection;

use crate::session::store::State;

/// A provisioned Mod host for one test. It holds the process-wide runtime-kit scope
/// (`BESKID_RUNTIME_PREFIX` names the shared debug kit) for as long as the test prepares
/// documents, because Mod qualification and loading resolve the kit from the environment.
pub(crate) struct ProvisionedModHost {
    mod_host: ModHostSelection,
    _scope: ModHostScope,
}

impl ProvisionedModHost {
    /// Provision every native Mod of the project that owns `source_path` and select the
    /// provisioning CLI as the Mod host.
    pub(crate) fn for_source(source_path: &Path) -> Self {
        let scope = ModHostScope::enter().expect("enter the shared runtime-kit scope");
        let context = beskid_analysis::CompilationContext::try_for_analysis_path(source_path, None)
            .unwrap_or_else(|| panic!("compilation context for {}", source_path.display()));
        let plan = context.compile_plan.as_ref().expect("project-backed compile plan");
        scope.corelib_mod_invoker(plan).expect("provision the project's native Mods");
        let cli = provisioned_cli().expect("workspace Mod host CLI");
        let mod_host = ModHostSelection::explicit(&cli).expect("valid Mod host executable");
        Self { mod_host, _scope: scope }
    }

    /// A fresh session whose Mod host is the provisioned CLI.
    pub(crate) fn state(&self) -> State {
        State { mod_host: self.mod_host.clone(), ..State::default() }
    }
}
