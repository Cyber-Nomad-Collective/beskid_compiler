//! The executable front end: the syntax and composition products codegen lowers.

/// Result of the shared front-end spine for one entry: the expanded syntax assembly after Mod
/// rewriting and the composition authority of that exact syntax generation.
///
/// It carries no types. Typing is the query layer's (`beskid_queries`): codegen builds its
/// `TypedProgram` from [`Self::syntax_assembly`] and reads every semantic fact from the
/// generation-bound queries, the same facts the `beskid check` gate judged before this value
/// existed. The legality gate (`check_items`) runs again at emission over the reachable items.
pub struct ExecutableFrontEnd {
    pub assembly: crate::projects::ProgramAssembly,
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
}

impl std::fmt::Debug for ExecutableFrontEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutableFrontEnd")
            .field("assembly", &self.assembly)
            .field("binding_plan", &self.binding_plan)
            .finish_non_exhaustive()
    }
}

impl ExecutableFrontEnd {
    /// Return the syntax-only authority for the post-mod-rewrite frontend snapshot.
    ///
    /// `ProgramAssembly` retains parsed source units, while `program` is the expanded/re-written
    /// entry consumed by generation-safe semantic facts. Replacing exactly that unit keeps syntax
    /// item keys aligned.
    pub fn syntax_assembly(&self) -> crate::projects::ProgramAssembly {
        self.assembly.with_entry_program(self.program.clone(), self.assembly.generation)
    }
}

/// Options for the front end of a prepare.
#[derive(Debug, Clone)]
pub struct FrontEndOptions {
    pub with_semantic_diagnostics: bool,
    pub assembly_discovery: crate::projects::AssemblyDiscovery,
    pub assembly_recovery: crate::projects::AssemblyRecoveryPolicy,
    pub module_level_meta_items_allowed: Option<bool>,
}

impl Default for FrontEndOptions {
    fn default() -> Self {
        Self {
            with_semantic_diagnostics: true,
            assembly_discovery: crate::projects::AssemblyDiscovery::ImportClosure,
            assembly_recovery: crate::projects::AssemblyRecoveryPolicy::Strict,
            module_level_meta_items_allowed: None,
        }
    }
}
