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
use crate::mod_host::{
    ModHostInput, SyntaxFix, native_invoker_for_plan, run_analyze_rewrite_with_invoker, run_through_generate,
    run_through_generate_without_materializing_outputs,
};
use crate::projects::{
    AssemblyRootSet, CompilePlan, PreparedProjectWorkspace, ProgramAssembly, assemble_program,
    assembly_options_for_prepare,
};

use super::super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::super::entry_session::{
    cached_executable_if_valid, store_executable_and_snapshot, update_semantic_snapshot,
};
use super::super::front_end::FrontEndTypedResult;
use super::super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program_with_pipeline};
use super::super::semantic_facts::{DependencyTypingPolicy, resolve_and_type_program_with_assembly};
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
    mut fact_authority: Option<&mut dyn SemanticFactAuthority>,
) -> Result<PrepareSpineOutput> {
    // Injected executor identity is not part of a reusable session cache key.
    // A native Mod adapter closure is a different assembly than the session's ordinary one.
    let use_session_cache =
        use_session_cache && options.mod_invoker.is_none() && !options.native_mod_adapter_sources;
    let mut assembly_options = assembly_options_for_prepare(plan, options.front_end.assembly_discovery);
    assembly_options.recovery_policy = options.front_end.assembly_recovery;
    assembly_options.native_mod_adapter_sources = options.native_mod_adapter_sources;

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
        if cached.recovery_policy != assembly_options.recovery_policy {
            anyhow::bail!("cached syntax assembly recovery policy does not match the requested preparation policy");
        }
        cached.clone()
    } else {
        observe_phase_result(pipeline, PROGRAM_ASSEMBLE, || {
            assemble_program(plan, prepared_workspace, entry_path, Some(entry_source), &assembly_options, pipeline)
                .map_err(|err| anyhow::anyhow!("{err}"))
        })?
    };
    let mut assembly = if use_session_cache {
        (*session_for_assembly(session_fingerprint.clone(), assembled)?.assembly).clone()
    } else {
        assembled
    };

    // An entry-less library has no entry file: the caller's entry source is the empty placeholder.
    // Its entry unit is the first own root unit, which renders against its own assembled source.
    let own_entry_source;
    let entry_source = if matches!(assembly.root_set, AssemblyRootSet::OwnUnits(_)) {
        own_entry_source = assembly.entry_unit().source.clone();
        own_entry_source.as_str()
    } else {
        entry_source
    };
    let run_rules = options.front_end.with_semantic_diagnostics || collect_diagnostics;

    if use_session_cache && let Some(cached) = cached_executable_if_valid(&session_fingerprint, assembly.generation) {
        let syntax_generation_id = assembly.generation.0;
        Span::current().record("syntax_generation_id", syntax_generation_id);
        let front = cached.as_ref();
        let mut fact_diagnostics = Vec::new();
        if let Some(authority) = fact_authority.as_mut() {
            let findings = authority.fact_findings(&front.assembly, &front.program)?;
            let facts = fact_findings_to_diagnostics(&front.assembly, entry_source, findings)?;
            if collect_diagnostics {
                fact_diagnostics = facts;
            } else {
                require_no_semantic_errors(&facts)?;
            }
        }
        // A cached executable proves only the entry unit; every other root is judged again. The
        // query gate above has already judged every root, so its findings merge into the
        // legacy root diagnostics the same way they merge on the uncached path.
        let roots = additional_root_diagnostics(&front.assembly, options, run_rules, fact_authority.is_some())?;
        let mut diagnostics = Vec::new();
        if collect_diagnostics {
            diagnostics = roots;
            merge_fact_diagnostics(&mut diagnostics, fact_diagnostics);
            diagnostics = dedupe_diagnostics(diagnostics);
        } else {
            require_no_semantic_errors(&roots)?;
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

    let entry_unit = assembly.entry_unit().clone();
    let mut program = entry_unit.program.clone();
    observe_phase(pipeline, PARSE, || {});

    let native_invoker = if options.mod_invoker.is_none() { native_invoker_for_plan(plan, pipeline)? } else { None };
    let invoker_ref = options
        .mod_invoker
        .as_deref()
        .or_else(|| native_invoker.as_ref().map(|invoker| invoker as &dyn crate::mod_host::ContractInvoker));

    let mod_input = ModHostInput {
        semantic_scope: fact_authority.as_ref().and_then(|authority| authority.mod_scope()),
        semantic_authority: None,
        compile_plan: Some(plan),
        source_name: &entry_unit.logical_name,
        source: entry_source,
        pipeline,
        invoker: invoker_ref,
        cached_target_fingerprint: None,
        syntax_generation_id: Some(assembly.generation),
    };
    let mut generated = if use_session_cache {
        run_through_generate(program.clone(), &mod_input)?
    } else {
        run_through_generate_without_materializing_outputs(program.clone(), &mod_input)?
    };
    program = generated.program;
    if let Some(scope) = mod_input.semantic_scope {
        assembly = scope.with_authority(Some(&program), &mut |_| Ok(()))?;
        if use_session_cache {
            session_for_assembly(session_fingerprint.clone(), assembly.clone())?;
        }
    }

    assembly
        .compiled_mod_metadata
        .extend(generated.generator_outcomes.iter().flat_map(|outcome| outcome.compiled_metadata.iter().cloned()));
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
    rule_options.defer_try_diagnostics = fact_authority.is_some();

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

    let mut composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;
    composition_result.snapshot.source_unit_path = Some(entry_unit.path.clone());

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

    let mod_snapshot = if use_session_cache {
        super::super::session::cached_semantic_snapshot(&session_fingerprint)
    } else {
        local_semantic_snapshot.clone()
    };
    let mod_rewrite = run_analyze_rewrite_with_invoker(
        program.clone(),
        &generated.session,
        invoker_ref,
        Some(&mod_input),
        mod_snapshot.as_ref(),
        pipeline,
    )?;
    program = mod_rewrite.program;
    if let Some(scope) = mod_input.semantic_scope {
        let retained_metadata = assembly.compiled_mod_metadata.clone();
        assembly = scope.with_authority(Some(&program), &mut |_| Ok(()))?;
        for metadata in retained_metadata {
            if !assembly.compiled_mod_metadata.contains(&metadata) {
                assembly.compiled_mod_metadata.push(metadata);
            }
        }
        if use_session_cache {
            session_for_assembly(session_fingerprint.clone(), assembly.clone())?;
        }
    }
    drop(mod_input);
    let syntax_generation_id = assembly.generation.0;

    // Semantic-fact findings (invalid `?` targets and the reachability-scoped legality facts the
    // lowering gate evaluates) for the entry's reachable items, possibly in dependency units. On
    // the diagnostics path they are merged after World A has run, so an entry-unit error both
    // authorities judge is reported once.
    let mut fact_diagnostics = Vec::new();
    if let Some(authority) = fact_authority.as_mut() {
        let findings = authority.fact_findings(&assembly, &program)?;
        let facts = fact_findings_to_diagnostics(&assembly, entry_source, findings)?;
        if collect_diagnostics {
            fact_diagnostics = facts;
        } else {
            require_no_semantic_errors(&facts)?;
        }
    }

    // Mod analyzer diagnostics are always collected so a mod `Error`-severity
    // diagnostic fails the typed/codegen build (mirrors `require_no_semantic_errors`
    // at the compiler-semantic gate above). On the diagnostics path they surface to
    // LSP; on the typed/codegen path `Warning`/`Note` are dropped, matching the
    // existing compiler-diagnostic contract.
    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
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

    // Entry-less libraries: every other own unit is a root too. Judge each before the entry's
    // executable is cached, so a root failure can never hide behind a cached entry.
    let root_diagnostics = additional_root_diagnostics(&assembly, options, run_rules, fact_authority.is_some())?;
    if collect_diagnostics {
        collected_diagnostics.extend(root_diagnostics);
    } else {
        require_no_semantic_errors(&root_diagnostics)?;
    }

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

    merge_fact_diagnostics(&mut collected_diagnostics, fact_diagnostics);
    Ok(PrepareSpineOutput {
        prepared: PreparedCompilation { assembly, program, binding_plan, composition_snapshot, typed },
        collected_diagnostics: dedupe_diagnostics(collected_diagnostics),
        collected_fixes: analyzer_fixes,
    })
}

/// Rule, resolution, and type diagnostics for every root unit other than the entry unit.
///
/// Only an entry-less library assembly ([`AssemblyRootSet::OwnUnits`]) has such roots. Each one
/// is judged as its own entry against the same assembly, so its errors are reported in its own
/// source, while the other units stay dependencies whose errors are not reported against it.
/// Dependency bodies are not re-typed per root ([`DependencyTypingPolicy::EntryOnly`]): their
/// errors are discarded by the checker anyway, and re-typing every body for every root is
/// quadratic. Fails closed when the root set names a unit outside the assembly.
fn additional_root_diagnostics(
    assembly: &ProgramAssembly,
    options: &PrepareOptions,
    run_rules: bool,
    defer_try_diagnostics: bool,
) -> Result<Vec<SemanticDiagnostic>> {
    let roots = assembly
        .additional_root_indices()
        .ok_or_else(|| anyhow::anyhow!("assembly root set names a source unit outside the prepared assembly"))?;
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let known_module_paths = assembly.module_index.known_module_path_strings();
    let mut diagnostics = Vec::new();
    for index in roots {
        let root_assembly = assembly
            .with_entry_index(index)
            .ok_or_else(|| anyhow::anyhow!("assembly root unit {index} is outside the prepared assembly"))?;
        let unit = &assembly.units[index];
        if run_rules {
            let mut rule_options = AnalysisOptions::default();
            rule_options.module_level_meta_items_allowed = options.front_end.module_level_meta_items_allowed;
            rule_options.known_assembly_module_paths = Some(known_module_paths.clone());
            rule_options.program_assembly_module_index = Some((*assembly.module_index).clone());
            rule_options.entry_source_path = Some(unit.path.clone());
            rule_options.program_assembly = Some(root_assembly.clone());
            rule_options.defer_try_diagnostics = defer_try_diagnostics;
            diagnostics.extend(semantic_rule_diagnostics_for_program_with_pipeline(
                &unit.program.node,
                unit.logical_name.clone(),
                &unit.source,
                rule_options,
                None,
            ));
        }
        if let Err(error) = resolve_and_type_program_with_assembly(
            &unit.program,
            Some(&root_assembly),
            None,
            DependencyTypingPolicy::EntryOnly,
        ) {
            diagnostics.extend(semantic_facts_errors_to_diagnostics(error, &unit.logical_name, &unit.source));
        }
    }
    Ok(diagnostics)
}
