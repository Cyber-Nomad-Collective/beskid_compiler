//! Shared front-end spine: assembly, parse, mods, semantic gate, HIR with module index.

use std::path::Path;

use anyhow::Result;
use beskid_pipeline::PipelineObserver;

use crate::projects::{CompilePlan, PreparedProjectWorkspace};

use super::prepare::{PrepareOptions, prepare_compilation};

/// Result of the shared front-end through typed HIR (codegen consumes this).
pub struct FrontEndTypedResult {
    pub assembly: crate::projects::ProgramAssembly,
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    pub hir: crate::syntax::Spanned<crate::hir::HirProgram>,
    pub resolution: crate::resolve::Resolution,
    pub typed: crate::types::TypeResult,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
}

impl std::fmt::Debug for FrontEndTypedResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrontEndTypedResult")
            .field("assembly", &self.assembly)
            .field("resolution", &self.resolution)
            .field("typed", &self.typed)
            .finish_non_exhaustive()
    }
}

impl FrontEndTypedResult {
    /// Return the syntax-only authority for the post-mod-rewrite frontend snapshot.
    ///
    /// `ProgramAssembly` retains parsed source units for HIR compatibility, while `program`
    /// is the expanded/re-written entry consumed by generation-safe semantic facts. Replacing
    /// exactly that unit keeps syntax item keys aligned without carrying legacy HIR units.
    pub fn syntax_assembly(&self) -> crate::projects::SyntaxProgramAssembly {
        let assembly = &self.assembly;
        let mut units = assembly.units.as_ref().clone();
        units[assembly.entry_index].program = self.program.clone();
        let mut syntax = crate::projects::SyntaxProgramAssembly::new(
            assembly.roots.clone(),
            std::sync::Arc::new(units),
            assembly.entry_index,
            assembly.discovery,
            std::sync::Arc::clone(&assembly.module_index),
            assembly.has_std_dependency,
        );
        syntax.set_trusted_corelib_service_paths_for_project_assembly(std::sync::Arc::clone(
            &assembly.trusted_corelib_service_paths,
        ));
        syntax
    }
}

/// Options for [`compile_front_end_with_pipeline`].
#[derive(Debug, Clone)]
pub struct FrontEndOptions {
    pub with_semantic_diagnostics: bool,
    pub assembly_discovery: crate::projects::AssemblyDiscovery,
    pub module_level_meta_items_allowed: Option<bool>,
}

impl Default for FrontEndOptions {
    fn default() -> Self {
        Self {
            with_semantic_diagnostics: true,
            assembly_discovery: crate::projects::AssemblyDiscovery::ImportClosure,
            module_level_meta_items_allowed: None,
        }
    }
}

/// Assemble, run mod host + semantic gate, and lower the entry unit with cross-module resolution.
pub fn compile_front_end_with_pipeline(
    entry_path: &Path,
    entry_source: &str,
    compile_plan: Option<&CompilePlan>,
    prepared_workspace: Option<&PreparedProjectWorkspace>,
    options: FrontEndOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<FrontEndTypedResult> {
    let plan = compile_plan.ok_or_else(|| {
        anyhow::anyhow!("compile_front_end requires a CompilePlan (project context)")
    })?;

    let resolved = super::prepare::resolved_input_from_plan(
        entry_path.to_path_buf(),
        entry_source.to_string(),
        plan.clone(),
        prepared_workspace.cloned(),
        None,
    );

    let prepared = prepare_compilation(
        &resolved,
        PrepareOptions {
            front_end: options,
            ..Default::default()
        },
        pipeline,
    )?;

    prepared.into_executable()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{FrontEndOptions, compile_front_end_with_pipeline};
    use crate::services::{parse_program_with_source_name, synthetic_compile_plan_for_source};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn syntax_assembly_replaces_only_the_entry_program_after_mod_rewrite() {
        let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("beskid_front_end_syntax_assembly_{test_id}"));
        std::fs::create_dir_all(&root).expect("test source root");
        let entry_path = root.join("Main.bd");
        let source = "i32 Main() { return 0; }";
        std::fs::write(&entry_path, source).expect("entry source");

        let plan = synthetic_compile_plan_for_source(&entry_path);
        let mut front = compile_front_end_with_pipeline(
            &entry_path,
            source,
            Some(&plan),
            None,
            FrontEndOptions::default(),
            None,
        )
        .expect("front end");
        let rewritten = parse_program_with_source_name("Main.bd", "i32 Rewritten() { return 0; }")
            .expect("rewritten entry program");
        front.program = rewritten.clone();

        let syntax_assembly = front.syntax_assembly();

        assert_eq!(syntax_assembly.entry_unit().program, rewritten);
        assert_eq!(syntax_assembly.roots(), &front.assembly.roots);
        assert_eq!(syntax_assembly.entry_index(), front.assembly.entry_index);
        assert_eq!(syntax_assembly.discovery(), front.assembly.discovery);
        assert!(std::sync::Arc::ptr_eq(
            syntax_assembly.module_index(),
            &front.assembly.module_index,
        ));
        assert_eq!(
            syntax_assembly.has_std_dependency(),
            front.assembly.has_std_dependency,
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
