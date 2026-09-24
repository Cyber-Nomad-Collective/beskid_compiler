//! The single front-end prepare spine shared by every entry point.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{COMPOSITION_RESOLVE, LOWER, LOWER_READY, PARSE, PROGRAM_ASSEMBLE, SEMANTIC, SEMANTIC_SNAPSHOT},
};
use tracing::Span;

use crate::AnalysisOptions;
use crate::analysis::SemanticDiagnostic;
use crate::analysis::rules::RuleContext;
use crate::mod_host::{
    ModHostInput, SyntaxFix, native_invoker_for_plan, run_analyze_rewrite_after_composition,
    run_analyze_rewrite_with_invoker, run_through_generate, run_through_generate_without_materializing_outputs,
};
use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProgramAssembly, assemble_program, assembly_options_for_prepare,
};

use super::super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::super::entry_session::{
    cached_executable_if_valid, store_executable_and_snapshot, update_semantic_snapshot,
};
use super::super::front_end::FrontEndTypedResult;
use super::super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program_with_pipeline};
use super::super::semantic_facts::resolve_and_type_program_with_assembly;
use super::super::session::{SemanticSnapshot, SessionFingerprint, session_for_assembly};
use super::*;

pub(super) struct PrepareSpineOutput {
    pub(super) prepared: PreparedCompilation,
    pub(super) collected_diagnostics: Vec<SemanticDiagnostic>,
    pub(super) collected_fixes: Vec<SyntaxFix>,
}

pub(super) fn run_prepare_spine(
    entry_path: &Path,
    entry_source: &str,
    plan: &CompilePlan,
    prepared_workspace: Option<&PreparedProjectWorkspace>,
    cached_assembly: Option<&ProgramAssembly>,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
    collect_diagnostics: bool,
    use_session_cache: bool,
    mut try_authority: Option<&mut TryDiagnosticAuthority<'_>>,
) -> Result<PrepareSpineOutput> {
    let assembly_options = assembly_options_for_prepare(plan, options.front_end.assembly_discovery);

    let session_fingerprint = SessionFingerprint::for_entry(plan, entry_path);
    let _prepare_guard = tracing::info_span!(
        target: "beskid.analysis",
        "beskid.analysis.prepare",
        entry = %entry_path.display(),
        session_fingerprint = %session_fingerprint_field(&session_fingerprint),
        syntax_generation_id = tracing::field::Empty,
    )
    .entered();

    // Admit the caller's actual source generation before consulting any cached
    // executable. The entry fingerprint identifies a session, not its revision.
    let assembled = if let Some(cached) = cached_assembly {
        cached.clone()
    } else {
        observe_phase_result(pipeline, PROGRAM_ASSEMBLE, || {
            assemble_program(plan, prepared_workspace, entry_path, Some(entry_source), &assembly_options, pipeline)
                .map_err(|err| anyhow::anyhow!("{err}"))
        })?
    };
    let assembly = if use_session_cache {
        (*session_for_assembly(session_fingerprint.clone(), assembled)?.assembly).clone()
    } else {
        assembled
    };

    if use_session_cache && let Some(cached) = cached_executable_if_valid(&session_fingerprint, assembly.generation) {
        let syntax_generation_id = assembly.generation.0;
        Span::current().record("syntax_generation_id", syntax_generation_id);
        let front = cached.as_ref();
        let mut diagnostics = Vec::new();
        if let Some(authority) = try_authority.as_mut() {
            let mut ctx = RuleContext::new(
                front.assembly.entry_unit().logical_name.clone(),
                entry_source,
                AnalysisOptions::default(),
            );
            for span in authority(&front.assembly, &front.program)? {
                ctx.emit_issue(span, crate::analysis::diagnostic_kinds::SemanticIssueKind::TypeInvalidTryTarget);
            }
            if collect_diagnostics {
                diagnostics = ctx.diagnostics;
            } else {
                require_no_semantic_errors(&ctx.diagnostics)?;
            }
        }
        return Ok(PrepareSpineOutput {
            prepared: PreparedCompilation {
                assembly: front.assembly.clone(),
                program: front.program.clone(),
                binding_plan: front.binding_plan.clone(),
                composition_snapshot: front.composition_snapshot.clone(),
                typed: Some(cached),
            },
            collected_diagnostics: diagnostics,
            collected_fixes: Vec::new(),
        });
    }

    let entry_unit = assembly.entry_unit();
    let mut program = entry_unit.program.clone();
    observe_phase(pipeline, PARSE, || {});

    let native_invoker = native_invoker_for_plan(plan, pipeline).ok().flatten();
    let invoker_ref = native_invoker.as_ref().map(|invoker| invoker as &dyn crate::mod_host::ContractInvoker);

    let mod_input = ModHostInput {
        compile_plan: Some(plan),
        source_name: &entry_unit.logical_name,
        source: entry_source,
        pipeline,
        invoker: invoker_ref,
        cached_target_fingerprint: None,
    };
    let mut generated = if use_session_cache {
        run_through_generate(program.clone(), &mod_input)?
    } else {
        run_through_generate_without_materializing_outputs(program.clone(), &mod_input)?
    };
    program = generated.program;

    let mut collected_diagnostics = generated.macro_diagnostics;
    let syntax_generation_id = assembly.generation.0;
    Span::current().record("syntax_generation_id", syntax_generation_id);
    let mut local_semantic_snapshot = None;

    let mut rule_options = AnalysisOptions::default();
    rule_options.module_level_meta_items_allowed = options.front_end.module_level_meta_items_allowed;
    rule_options.known_assembly_module_paths = Some(assembly.module_index.known_module_path_strings());
    rule_options.program_assembly_module_index = Some((*assembly.module_index).clone());
    rule_options.entry_source_path = Some(entry_unit.path.clone());
    rule_options.program_assembly = Some(assembly.clone());
    rule_options.defer_try_diagnostics = try_authority.is_some();

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
            let snapshot = SemanticSnapshot::from_diagnostics(snapshot_diagnostics, syntax_generation_id, "semantic");
            if use_session_cache {
                update_semantic_snapshot(&session_fingerprint, snapshot);
            } else {
                local_semantic_snapshot = Some(snapshot);
            }
        });
    }

    let composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;

    if options.front_end.with_semantic_diagnostics || collect_diagnostics {
        if use_session_cache {
            if let Some(mut snapshot) = super::super::session::cached_semantic_snapshot(&session_fingerprint) {
                snapshot = snapshot.with_composition(&composition_result.snapshot);
                update_semantic_snapshot(&session_fingerprint, snapshot);
            }
        } else if let Some(snapshot) = local_semantic_snapshot.take() {
            local_semantic_snapshot = Some(snapshot.with_composition(&composition_result.snapshot));
        }
    }

    let mod_rewrite = if use_session_cache {
        run_analyze_rewrite_after_composition(
            program.clone(),
            &generated.session,
            &session_fingerprint,
            invoker_ref,
            pipeline,
        )?
    } else {
        run_analyze_rewrite_with_invoker(
            program.clone(),
            &generated.session,
            invoker_ref,
            None,
            local_semantic_snapshot.as_ref(),
            pipeline,
        )?
    };
    program = mod_rewrite.program;

    if let Some(authority) = try_authority.as_mut() {
        let spans = authority(&assembly, &program)?;
        let mut ctx = RuleContext::new(entry_unit.logical_name.clone(), entry_source, rule_options.clone());
        for span in spans {
            ctx.emit_issue(span, crate::analysis::diagnostic_kinds::SemanticIssueKind::TypeInvalidTryTarget);
        }
        if collect_diagnostics {
            collected_diagnostics.extend(ctx.diagnostics);
        } else {
            require_no_semantic_errors(&ctx.diagnostics)?;
        }
    }

    // Mod analyzer diagnostics are always collected so a mod `Error`-severity
    // diagnostic fails the typed/codegen build (mirrors `require_no_semantic_errors`
    // at the compiler-semantic gate above). On the diagnostics path they surface to
    // LSP; on the typed/codegen path `Warning`/`Note` are dropped, matching the
    // existing compiler-diagnostic contract.
    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, entry_unit, entry_source);
    // Mod quick-fixes are an IDE concern — collect them only on the diagnostics path
    // (the typed/codegen build path does not need fixes).
    let analyzer_fixes =
        if collect_diagnostics { collect_analyzer_fixes(&mod_rewrite.analyzer_outcomes) } else { Vec::new() };
    if collect_diagnostics {
        collected_diagnostics.extend(analyzer_diagnostics);
    } else {
        require_no_semantic_errors(&analyzer_diagnostics)?;
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

    generated.session.set_composition_snapshot(composition_result.snapshot.clone());

    let binding_plan = composition_result.plan.clone();
    let composition_snapshot = composition_result.snapshot.clone();

    observe_phase(pipeline, LOWER_READY, || {});

    let typed = match observe_phase_result(pipeline, LOWER, || {
        resolve_and_type_program_with_assembly(&program, Some(&assembly), pipeline, options.dependency_typing)
    }) {
        Ok((program, resolution, typed)) => {
            let resolution_fingerprint = typed_fingerprint(&resolution);
            let types_fingerprint = typed_fingerprint_types(&typed);
            let typed_result = FrontEndTypedResult {
                assembly: assembly.clone(),
                program,
                resolution,
                typed,
                binding_plan: binding_plan.clone(),
                composition_snapshot: composition_snapshot.clone(),
            };
            let executable_snapshot = (if use_session_cache {
                super::super::session::cached_semantic_snapshot(&session_fingerprint)
            } else {
                local_semantic_snapshot.clone()
            })
            .map(|snap| snap.with_typed_resolution(resolution_fingerprint, types_fingerprint))
            .unwrap_or_else(|| {
                SemanticSnapshot::from_diagnostics(&[], syntax_generation_id, "executable")
                    .with_composition(&composition_snapshot)
                    .with_typed_resolution(resolution_fingerprint, types_fingerprint)
            });
            if use_session_cache {
                let stored =
                    store_executable_and_snapshot(&session_fingerprint, Some(typed_result), executable_snapshot)
                        .ok_or_else(|| {
                            anyhow::anyhow!("entry session missing or changed before executable cache store")
                        })?;
                Some(stored)
            } else {
                Some(Arc::new(typed_result))
            }
        }
        Err(error) if collect_diagnostics => {
            collected_diagnostics.extend(semantic_facts_errors_to_diagnostics(
                error,
                entry_unit.logical_name.as_str(),
                entry_source,
            ));
            None
        }
        Err(error) => return Err(error.into()),
    };

    Ok(PrepareSpineOutput {
        prepared: PreparedCompilation { assembly, program, binding_plan, composition_snapshot, typed },
        collected_diagnostics: dedupe_diagnostics(collected_diagnostics),
        collected_fixes: analyzer_fixes,
    })
}
