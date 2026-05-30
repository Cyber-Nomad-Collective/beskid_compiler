//! Unified compilation prepare spine consumed by analyze, run, build, test, and LSP.

use std::path::Path;

use anyhow::Result;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{
        COMPOSITION_RESOLVE, LOWER, LOWER_READY, MOD_REWRITE, PARSE, PROGRAM_ASSEMBLE, SEMANTIC,
        SEMANTIC_SNAPSHOT,
    },
};

use crate::analysis::SemanticDiagnostic;
use crate::projects::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, PreparedProjectWorkspace, ProgramAssembly,
    assemble_program,
};
use crate::syntax::Spanned;
use crate::AnalysisOptions;
use crate::mod_host::{ModHostInput, run_analyze_rewrite, run_through_generate};

use super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::front_end::{FrontEndOptions, FrontEndTypedResult};
use super::input::ResolvedInput;
use super::lower::lower_normalize_resolve_type_spanned_with_assembly;
use super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program};
use super::session::{
    SemanticSnapshot, SessionFingerprint, session_for_assembly, store_executable_on_session,
};

/// Whether prepare stops after diagnostics or continues through typed HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareMode {
    /// Semantic + composition on post-rewrite AST; no typed HIR (analyze / LSP gate).
    DiagnosticsOnly,
    /// Full front-end through typed HIR (run / build / test / codegen).
    Executable,
}

/// Options for [`prepare_compilation`].
#[derive(Debug, Clone)]
pub struct PrepareOptions {
    pub mode: PrepareMode,
    pub front_end: FrontEndOptions,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            mode: PrepareMode::Executable,
            front_end: FrontEndOptions::default(),
        }
    }
}

/// Result of the unified prepare spine (typed HIR present only in [`PrepareMode::Executable`]).
pub struct PreparedCompilation {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
    pub typed: Option<FrontEndTypedResult>,
}

impl PreparedCompilation {
    /// Typed HIR bundle for codegen; panics if prepare ran in diagnostics-only mode.
    pub fn into_executable(self) -> Result<FrontEndTypedResult> {
        self.typed.ok_or_else(|| {
            anyhow::anyhow!("prepare_compilation ran in DiagnosticsOnly mode; no typed HIR available")
        })
    }

    pub fn executable(&self) -> Result<&FrontEndTypedResult> {
        self.typed.as_ref().ok_or_else(|| {
            anyhow::anyhow!("prepare_compilation ran in DiagnosticsOnly mode; no typed HIR available")
        })
    }
}

/// Single front-end spine: assemble → mod host → rewrite → semantic → composition → (optional) typed HIR.
pub fn prepare_compilation(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)"))?;

    let spine = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        false,
    )?;

    Ok(spine.prepared)
}

/// Like [`prepare_compilation`], collecting semantic diagnostics instead of failing on errors.
pub fn prepare_compilation_diagnostics(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>)> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)"))?;

    let mut diagnostics = Vec::new();
    let spine = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        true,
    )?;
    diagnostics.extend(spine.collected_diagnostics);
    Ok((spine.prepared, diagnostics))
}

struct PrepareSpineOutput {
    prepared: PreparedCompilation,
    collected_diagnostics: Vec<SemanticDiagnostic>,
}

fn run_prepare_spine(
    entry_path: &Path,
    entry_source: &str,
    plan: &CompilePlan,
    prepared_workspace: Option<&PreparedProjectWorkspace>,
    cached_assembly: Option<&ProgramAssembly>,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
    collect_diagnostics: bool,
) -> Result<PrepareSpineOutput> {
    let mut assembly_options = AssemblyOptions::default();
    assembly_options.discovery = options.front_end.assembly_discovery;

    let session_fingerprint = SessionFingerprint::for_entry(plan, entry_path);

    let assembly = if let Some(cached) = cached_assembly {
        cached.clone()
    } else {
        observe_phase_result(pipeline, PROGRAM_ASSEMBLE, || {
            let assembled = assemble_program(
                plan,
                prepared_workspace,
                entry_path,
                Some(entry_source),
                &assembly_options,
            )
            .map_err(|err| anyhow::anyhow!("{err}"))?;
            let session = session_for_assembly(session_fingerprint.clone(), assembled);
            Ok::<crate::projects::ProgramAssembly, anyhow::Error>((*session.assembly).clone())
        })?
    };

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

    let mut collected_diagnostics = generated.macro_diagnostics;

    program = observe_phase_result(pipeline, MOD_REWRITE, || {
        run_analyze_rewrite(program, &generated.session, pipeline)
    })?;

    let mut rule_options = AnalysisOptions::default();
    rule_options.module_level_meta_items_allowed = options.front_end.module_level_meta_items_allowed;
    rule_options.known_assembly_module_paths =
        Some(assembly.module_index.known_module_path_strings());
    rule_options.program_assembly_module_index = Some((*assembly.module_index).clone());
    rule_options.entry_source_path = Some(entry_unit.path.clone());
    rule_options.program_assembly = Some(assembly.clone());
    rule_options.semantic_gate_only = options.mode == PrepareMode::DiagnosticsOnly;

    if options.front_end.with_semantic_diagnostics || collect_diagnostics {
        let semantic = observe_phase_result(pipeline, SEMANTIC, || {
            Ok::<_, anyhow::Error>(semantic_rule_diagnostics_for_program(
                &program.node,
                entry_unit.logical_name.clone(),
                entry_source,
                rule_options.clone(),
            ))
        })?;
        let snapshot_diagnostics = if collect_diagnostics {
            collected_diagnostics.extend(semantic);
            collected_diagnostics.as_slice()
        } else {
            require_no_semantic_errors(&semantic)?;
            semantic.as_slice()
        };
        observe_phase(pipeline, SEMANTIC_SNAPSHOT, || {
            store_executable_on_session(
                &session_fingerprint,
                None,
                SemanticSnapshot::from_diagnostics(snapshot_diagnostics),
            );
        });
    }

    let composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;

    if options.front_end.with_semantic_diagnostics && !collect_diagnostics {
        let composition_diagnostics = composition_result_to_diagnostics(
            &composition_result,
            program.span,
            entry_unit.logical_name.as_str(),
            entry_source,
            Some(plan),
        );
        require_no_semantic_errors(&composition_diagnostics)?;
    } else if collect_diagnostics {
        collected_diagnostics.extend(composition_result_to_diagnostics(
            &composition_result,
            program.span,
            entry_unit.logical_name.as_str(),
            entry_source,
            Some(plan),
        ));
    }

    generated
        .session
        .set_composition_snapshot(composition_result.snapshot.clone());

    let binding_plan = composition_result.plan.clone();
    let composition_snapshot = composition_result.snapshot.clone();

    let typed = if options.mode == PrepareMode::Executable {
        observe_phase(pipeline, LOWER_READY, || {});

        let (hir, resolution, typed) = observe_phase_result(pipeline, LOWER, || {
            lower_normalize_resolve_type_spanned_with_assembly(&program, Some(&assembly))
                .map_err(anyhow::Error::from)
        })?;

        Some(FrontEndTypedResult {
            assembly: assembly.clone(),
            program: program.clone(),
            hir,
            resolution,
            typed,
            binding_plan: binding_plan.clone(),
            composition_snapshot: composition_snapshot.clone(),
        })
    } else {
        None
    };

    Ok(PrepareSpineOutput {
        prepared: PreparedCompilation {
            assembly,
            program,
            binding_plan,
            composition_snapshot,
            typed,
        },
        collected_diagnostics,
    })
}

/// Build a [`ResolvedInput`] from paths for analyze/LSP when only a compile plan is available.
pub fn resolved_input_from_plan(
    source_path: std::path::PathBuf,
    source: String,
    compile_plan: CompilePlan,
    prepared_workspace: Option<PreparedProjectWorkspace>,
    assembly: Option<ProgramAssembly>,
) -> ResolvedInput {
    ResolvedInput {
        source_path,
        source,
        compile_plan: Some(compile_plan),
        prepared_workspace,
        workspace_summary: None,
        assembly,
    }
}
