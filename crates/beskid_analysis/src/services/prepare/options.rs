//! Prepare options and the prepared compilation result.

use std::sync::Arc;

use anyhow::Result;

use crate::projects::ProgramAssembly;
use crate::syntax::Spanned;

use super::super::front_end::{FrontEndOptions, FrontEndTypedResult};
use super::super::semantic_facts::DependencyTypingPolicy;

/// Options for [`prepare_compilation`].
#[derive(Clone)]
pub struct PrepareOptions {
    /// Producer-owned native service; borrowed semantic authority remains invocation-scoped.
    pub mod_invoker: Option<Arc<dyn crate::mod_host::ContractInvoker>>,
    pub front_end: FrontEndOptions,
    /// Whether dependency unit bodies are fully type-checked or only signatures prefetched.
    pub dependency_typing: DependencyTypingPolicy,
    /// Native Mod builds only: assemble the compiler-owned SDK adapter sources with the Mod
    /// (see `AssemblyOptions::native_mod_adapter_sources`).
    pub native_mod_adapter_sources: bool,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            mod_invoker: None,
            front_end: FrontEndOptions::default(),
            dependency_typing: DependencyTypingPolicy::FullClosure,
            native_mod_adapter_sources: false,
        }
    }
}

/// Result of the unified prepare spine (typed syntax when lower succeeds).
pub struct PreparedCompilation {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
    pub typed: Option<Arc<FrontEndTypedResult>>,
}

impl PreparedCompilation {
    /// Typed syntax bundle for codegen.
    pub fn into_executable(self) -> Result<FrontEndTypedResult> {
        let Some(typed) = self.typed else {
            return Err(anyhow::anyhow!(
                "prepare_compilation did not produce typed syntax (lower failed during diagnostic collection)"
            ));
        };
        Arc::try_unwrap(typed).map_err(|shared| {
            anyhow::anyhow!(
                "executable front-end is still shared in the entry session cache (strong_refs={})",
                Arc::strong_count(&shared)
            )
        })
    }

    pub fn executable(&self) -> Result<&FrontEndTypedResult> {
        self.typed.as_ref().map(|typed| typed.as_ref()).ok_or_else(|| {
            anyhow::anyhow!(
                "prepare_compilation did not produce typed syntax (lower failed during diagnostic collection)"
            )
        })
    }

    /// Syntax-only project assembly for generation-safe consumers (LSP, queries, ISLE).
    ///
    /// Prefer this over reading [`ProgramAssembly::units`]. When typed syntax exists, the
    /// post-mod-rewrite entry program is projected; otherwise the prepare-spine rewritten
    /// `program` replaces the entry unit. Callers must not fall back to
    /// `DocumentAnalysisSnapshot` for IDE authority.
    pub fn syntax_assembly(&self) -> crate::projects::ProgramAssembly {
        if let Some(typed) = self.typed.as_ref() {
            return typed.syntax_assembly();
        }
        let mut units = self.assembly.units.as_ref().clone();
        units[self.assembly.entry_index].program = self.program.clone();
        let mut syntax = crate::projects::ProgramAssembly::new(
            self.assembly.roots.clone(),
            Arc::new(units),
            self.assembly.entry_index,
            self.assembly.discovery,
            Arc::clone(&self.assembly.module_index),
            self.assembly.has_core_dependency,
            self.assembly.generation,
        )
        .with_recovery_policy(self.assembly.recovery_policy)
        .with_trusted_corelib_service_paths(Arc::clone(&self.assembly.trusted_corelib_service_paths))
        .with_glue_libraries(Arc::clone(&self.assembly.glue_libraries))
        .with_runtime_fixture(self.assembly.runtime_fixture.clone())
        .with_root_set(self.assembly.root_set.clone());
        syntax.verified_package_identities = self.assembly.verified_package_identities.clone();
        syntax
    }
}

impl std::fmt::Debug for PrepareOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrepareOptions")
            .field("front_end", &self.front_end)
            .field("dependency_typing", &self.dependency_typing)
            .field("mod_invoker", &self.mod_invoker.is_some())
            .field("native_mod_adapter_sources", &self.native_mod_adapter_sources)
            .finish()
    }
}
