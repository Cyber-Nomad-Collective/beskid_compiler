//! Unified compilation prepare spine consumed by analyze, run, build, test, and LSP.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tracing::Span;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{
        COMPOSITION_RESOLVE, LOWER, LOWER_READY, PARSE, PROGRAM_ASSEMBLE, SEMANTIC,
        SEMANTIC_SNAPSHOT,
    },
};

use crate::AnalysisOptions;
use crate::analysis::SemanticDiagnostic;
use crate::analysis::rules::{RuleContext, resolve, types};
use crate::mod_host::{ModHostInput, native_invoker_for_plan, run_analyze_rewrite_after_composition, run_through_generate};
use crate::mod_host::diagnostics::analyzer_diagnostic_to_semantic;
use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProgramAssembly, assemble_program,
    assembly_options_for_prepare,
};
use crate::syntax::Spanned;

use super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::entry_session::{
    cached_executable_if_valid, current_syntax_generation_id, store_executable_and_snapshot,
    update_semantic_snapshot,
};
use super::front_end::{FrontEndOptions, FrontEndTypedResult};
use super::input::ResolvedInput;
use super::lower::{
    DependencyTypingPolicy, LowerResolveTypeError,
    lower_normalize_resolve_type_spanned_with_assembly,
};
use super::semantic::{
    require_no_semantic_errors, semantic_rule_diagnostics_for_program_with_pipeline,
};
use super::session::{SemanticSnapshot, SessionFingerprint, session_for_assembly};

/// Options for [`prepare_compilation`].
#[derive(Debug, Clone)]
pub struct PrepareOptions {
    pub front_end: FrontEndOptions,
    /// Whether dependency unit bodies are fully type-checked or only signatures prefetched.
    pub dependency_typing: DependencyTypingPolicy,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            front_end: FrontEndOptions::default(),
            dependency_typing: DependencyTypingPolicy::FullClosure,
        }
    }
}

/// Result of the unified prepare spine (typed HIR when lower succeeds).
pub struct PreparedCompilation {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
    pub typed: Option<Arc<FrontEndTypedResult>>,
}

impl PreparedCompilation {
    /// Typed HIR bundle for codegen.
    pub fn into_executable(self) -> Result<FrontEndTypedResult> {
        let Some(typed) = self.typed else {
            return Err(anyhow::anyhow!(
                "prepare_compilation did not produce typed HIR (lower failed during diagnostic collection)"
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
        self.typed
            .as_ref()
            .map(|typed| typed.as_ref())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "prepare_compilation did not produce typed HIR (lower failed during diagnostic collection)"
                )
            })
    }
}

/// Single front-end spine: assemble → mod host → rewrite → semantic → composition → (optional) typed HIR.
pub fn prepare_compilation(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    let plan = resolved.compile_plan.as_ref().ok_or_else(|| {
        anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)")
    })?;

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

/// Like [`prepare_compilation`], collecting diagnostics from semantic, composition, and lower
/// instead of failing on errors.
pub fn prepare_compilation_diagnostics(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>)> {
    let plan = resolved.compile_plan.as_ref().ok_or_else(|| {
        anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)")
    })?;

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

fn session_fingerprint_field(fingerprint: &SessionFingerprint) -> String {
    format!(
        "{}:{}",
        fingerprint.entry_canonical.display(),
        fingerprint.lockfile_digest
    )
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
    let assembly_options =
        assembly_options_for_prepare(plan, options.front_end.assembly_discovery);

    let session_fingerprint = SessionFingerprint::for_entry(plan, entry_path);
    let _prepare_guard = tracing::info_span!(
        target: "beskid.analysis",
        "beskid.analysis.prepare",
        entry = %entry_path.display(),
        session_fingerprint = %session_fingerprint_field(&session_fingerprint),
        syntax_generation_id = tracing::field::Empty,
    )
    .entered();

    if let Some(cached) = cached_executable_if_valid(&session_fingerprint) {
        let syntax_generation_id = current_syntax_generation_id(&session_fingerprint);
        Span::current().record("syntax_generation_id", syntax_generation_id);
        let front = cached.as_ref();
        return Ok(PrepareSpineOutput {
            prepared: PreparedCompilation {
                assembly: front.assembly.clone(),
                program: front.program.clone(),
                binding_plan: front.binding_plan.clone(),
                composition_snapshot: front.composition_snapshot.clone(),
                typed: Some(cached),
            },
            collected_diagnostics: Vec::new(),
        });
    }

    let assembly = if let Some(cached) = cached_assembly {
        let session = session_for_assembly(session_fingerprint.clone(), cached.clone());
        (*session.assembly).clone()
    } else {
        observe_phase_result(pipeline, PROGRAM_ASSEMBLE, || {
            let assembled = assemble_program(
                plan,
                prepared_workspace,
                entry_path,
                Some(entry_source),
                &assembly_options,
                pipeline,
            )
            .map_err(|err| anyhow::anyhow!("{err}"))?;
            let session = session_for_assembly(session_fingerprint.clone(), assembled);
            Ok::<crate::projects::ProgramAssembly, anyhow::Error>((*session.assembly).clone())
        })?
    };

    let entry_unit = assembly.entry_unit();
    let mut program = entry_unit.program.clone();
    observe_phase(pipeline, PARSE, || {});

    let native_invoker = native_invoker_for_plan(plan, pipeline).ok().flatten();
    let invoker_ref = native_invoker.as_ref().map(|invoker| invoker as &dyn crate::mod_host::ContractInvoker);

    let mut generated = run_through_generate(
        program.clone(),
        &ModHostInput {
            compile_plan: Some(plan),
            source_name: &entry_unit.logical_name,
            source: entry_source,
            pipeline,
            invoker: invoker_ref,
            cached_target_fingerprint: None,
        },
    )?;
    program = generated.program;

    let mut collected_diagnostics = generated.macro_diagnostics;
    let syntax_generation_id = current_syntax_generation_id(&session_fingerprint);
    Span::current().record("syntax_generation_id", syntax_generation_id);

    let mut rule_options = AnalysisOptions::default();
    rule_options.module_level_meta_items_allowed =
        options.front_end.module_level_meta_items_allowed;
    rule_options.known_assembly_module_paths =
        Some(assembly.module_index.known_module_path_strings());
    rule_options.program_assembly_module_index = Some((*assembly.module_index).clone());
    rule_options.entry_source_path = Some(entry_unit.path.clone());
    rule_options.program_assembly = Some(assembly.clone());

    if options.front_end.with_semantic_diagnostics || collect_diagnostics {
        let semantic = observe_phase_result(pipeline, SEMANTIC, || {
            Ok::<_, anyhow::Error>(semantic_rule_diagnostics_for_program_with_pipeline(
                &program.node,
                entry_unit.logical_name.clone(),
                entry_source,
                rule_options.clone(),
                pipeline,
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
            update_semantic_snapshot(
                &session_fingerprint,
                SemanticSnapshot::from_diagnostics(
                    snapshot_diagnostics,
                    syntax_generation_id,
                    "semantic",
                ),
            );
        });
    }

    let composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;

    if (options.front_end.with_semantic_diagnostics || collect_diagnostics)
        && let Some(mut snapshot) = super::session::cached_semantic_snapshot(&session_fingerprint)
    {
        snapshot = snapshot.with_composition(&composition_result.snapshot);
        update_semantic_snapshot(&session_fingerprint, snapshot);
    }

    let mod_rewrite = run_analyze_rewrite_after_composition(
        program.clone(),
        &generated.session,
        &session_fingerprint,
        invoker_ref,
        pipeline,
    )?;
    program = mod_rewrite.program;

    if collect_diagnostics {
        for outcome in &mod_rewrite.analyzer_outcomes {
            for diagnostic in &outcome.diagnostics {
                collected_diagnostics.push(analyzer_diagnostic_to_semantic(
                    diagnostic,
                    outcome.type_id.as_str(),
                    entry_unit.logical_name.as_str(),
                    entry_source,
                ));
            }
        }
    }

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

    observe_phase(pipeline, LOWER_READY, || {});

    let typed = match observe_phase_result(pipeline, LOWER, || {
        lower_normalize_resolve_type_spanned_with_assembly(
            &program,
            Some(&assembly),
            pipeline,
            options.dependency_typing,
        )
    }) {
        Ok((hir, resolution, typed)) => {
            let resolution_fingerprint = typed_fingerprint(&resolution);
            let types_fingerprint = typed_fingerprint_types(&typed);
            let typed_result = FrontEndTypedResult {
                assembly: assembly.clone(),
                program: program.clone(),
                hir,
                resolution,
                typed,
                binding_plan: binding_plan.clone(),
                composition_snapshot: composition_snapshot.clone(),
            };
            let executable_snapshot = super::session::cached_semantic_snapshot(&session_fingerprint)
                .map(|snap| snap.with_typed_resolution(resolution_fingerprint, types_fingerprint))
                .unwrap_or_else(|| {
                    SemanticSnapshot::from_diagnostics(&[], syntax_generation_id, "executable")
                        .with_composition(&composition_snapshot)
                        .with_typed_resolution(resolution_fingerprint, types_fingerprint)
                });
            let stored = store_executable_and_snapshot(
                &session_fingerprint,
                Some(typed_result),
                executable_snapshot,
            )
            .ok_or_else(|| anyhow::anyhow!("entry session missing for executable cache store"))?;
            Some(stored)
        }
        Err(error) if collect_diagnostics => {
            collected_diagnostics.extend(lower_errors_to_diagnostics(
                error,
                entry_unit.logical_name.as_str(),
                entry_source,
            ));
            None
        }
        Err(error) => return Err(error.into()),
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

fn lower_errors_to_diagnostics(
    error: LowerResolveTypeError,
    source_name: &str,
    source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut ctx = RuleContext::new(source_name, source, AnalysisOptions::default());
    match error {
        LowerResolveTypeError::Type(errors) => {
            for error in errors {
                types::emit_type_error(&mut ctx, error, None);
            }
        }
        LowerResolveTypeError::Resolve(errors) => {
            for error in errors {
                resolve::emit_resolve_error(&mut ctx, error);
            }
        }
        LowerResolveTypeError::Normalize(_) => {}
    }
    ctx.diagnostics
}

fn typed_fingerprint(resolution: &crate::resolve::Resolution) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    resolution.items.len().hash(&mut hasher);
    resolution.tables.resolved_values.len().hash(&mut hasher);
    hasher.finish()
}

fn typed_fingerprint_types(typed: &crate::types::TypeResult) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    typed.node_types.len().hash(&mut hasher);
    typed.lowering.cast_intents.len().hash(&mut hasher);
    hasher.finish()
}
