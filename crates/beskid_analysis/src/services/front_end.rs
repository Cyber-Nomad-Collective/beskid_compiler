//! Shared front-end spine: assembly, parse, mods, semantic gate, HIR with module index.

use std::path::Path;

use anyhow::Result;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{
        COMPOSITION_RESOLVE, LOWER, LOWER_READY, PARSE, PROGRAM_ASSEMBLE, SEMANTIC,
        SEMANTIC_SNAPSHOT,
    },
};

use crate::projects::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, PreparedProjectWorkspace, ProgramAssembly,
    assemble_program,
};
use crate::syntax::Spanned;

use super::input::ResolvedInput;
use super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::lower::lower_normalize_resolve_type_spanned_with_assembly;
use super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program};
use crate::AnalysisOptions;
use crate::mod_host::{ModHostInput, run_analyze_rewrite, run_through_generate};

/// Result of the shared front-end through typed HIR (codegen consumes this).
pub struct FrontEndTypedResult {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub hir: Spanned<crate::hir::HirProgram>,
    pub resolution: crate::resolve::Resolution,
    pub typed: crate::types::TypeResult,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
}

/// Options for [`compile_front_end_with_pipeline`].
#[derive(Debug, Clone)]
pub struct FrontEndOptions {
    pub with_semantic_diagnostics: bool,
    pub assembly_discovery: AssemblyDiscovery,
    pub module_level_meta_items_allowed: Option<bool>,
}

impl Default for FrontEndOptions {
    fn default() -> Self {
        Self {
            with_semantic_diagnostics: true,
            assembly_discovery: AssemblyDiscovery::ImportClosure,
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

    let mut assembly_options = AssemblyOptions::default();
    assembly_options.discovery = options.assembly_discovery;

    let assembly = observe_phase_result(pipeline, PROGRAM_ASSEMBLE, || {
        assemble_program(
            plan,
            prepared_workspace,
            entry_path,
            Some(entry_source),
            &assembly_options,
        )
        .map_err(|err| anyhow::anyhow!("{err}"))
    })?;

    let entry_unit = assembly.entry_unit();
    let mut program = entry_unit.program.clone();
    observe_phase(pipeline, PARSE, || {});

    let mut generated = run_through_generate(
        program.clone(),
        &ModHostInput {
            compile_plan: Some(plan),
            source_name: &entry_unit.logical_name,
            source: entry_source,
            pipeline,
            invoker: None,
        },
    )?;
    program = generated.program;

    if options.with_semantic_diagnostics {
        observe_phase_result(pipeline, SEMANTIC, || {
            let mut rule_options = AnalysisOptions::default();
            rule_options.module_level_meta_items_allowed = options.module_level_meta_items_allowed;
            rule_options.known_assembly_module_paths =
                Some(assembly.module_index.known_module_path_strings());
            let diagnostics = semantic_rule_diagnostics_for_program(
                &program.node,
                entry_unit.logical_name.clone(),
                entry_source,
                rule_options,
            );
            require_no_semantic_errors(&diagnostics).map_err(anyhow::Error::from)
        })?;
        observe_phase(pipeline, SEMANTIC_SNAPSHOT, || {});
    }
    let composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;
    if options.with_semantic_diagnostics {
        let composition_diagnostics = composition_result_to_diagnostics(
            &composition_result,
            program.span,
            entry_unit.logical_name.as_str(),
            entry_source,
            Some(plan),
        );
        require_no_semantic_errors(&composition_diagnostics).map_err(anyhow::Error::from)?;
    }
    generated
        .session
        .set_composition_snapshot(composition_result.snapshot.clone());

    program = run_analyze_rewrite(program, &generated.session, pipeline)?;

    observe_phase(pipeline, LOWER_READY, || {});

    let (hir, resolution, typed) = observe_phase_result(pipeline, LOWER, || {
        lower_normalize_resolve_type_spanned_with_assembly(&program, Some(&assembly))
            .map_err(anyhow::Error::from)
    })?;

    Ok(FrontEndTypedResult {
        assembly,
        program,
        hir,
        resolution,
        typed,
        binding_plan: composition_result.plan,
        composition_snapshot: composition_result.snapshot,
    })
}

/// Build typed HIR from a fully resolved [`ResolvedInput`] (CLI build/run path).
pub fn compile_front_end_from_resolved_input(
    resolved: &ResolvedInput,
    options: FrontEndOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<FrontEndTypedResult> {
    compile_front_end_with_pipeline(
        &resolved.source_path,
        &resolved.source,
        resolved.compile_plan.as_ref(),
        resolved.prepared_workspace.as_ref(),
        options,
        pipeline,
    )
}
