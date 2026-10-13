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
    ModHostInput, SyntaxFix, run_analyze_rewrite_with_host, run_through_generate,
    run_through_generate_without_materializing_outputs,
};
use crate::projects::{
    AssemblyRootSet, CompilePlan, PreparedProjectWorkspace, ProgramAssembly, TargetKind, assemble_program,
    assembly_options_for_prepare,
};

use super::super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::super::entry_session::{
    cached_executable_if_valid, store_executable_and_snapshot, update_semantic_snapshot,
};
use super::super::front_end::ExecutableFrontEnd;
use super::super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program_with_pipeline};
use super::super::session::{SemanticSnapshot, SessionFingerprint, session_for_assembly};
use super::*;

pub(super) struct PrepareSpineOutput {
    pub(super) prepared: PreparedCompilation,
    pub(super) collected_diagnostics: Vec<SemanticDiagnostic>,
    pub(super) collected_fixes: Vec<SyntaxFix>,
}

/// The one typing and legality authority of the spine is `fact_authority`, the generation-bound
/// query gate (`beskid_queries::semantic_diagnostics_for_roots`), which judges every own unit of
/// the project. Structural and style rules (`SemanticPipelineRule`) and Mod analyzer diagnostics
/// run beside it. When the gate accepts the program the spine assembles the executable front end
/// (`ExecutableFrontEnd`); nothing after the gate types the program a second time.
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
    fact_authority: &mut dyn SemanticFactAuthority,
) -> Result<PrepareSpineOutput> {
    // Injected executor identity is not part of a reusable session cache key.
    // A native Mod adapter closure is a different assembly than the session's ordinary one.
    let use_session_cache =
        use_session_cache && !options.mod_host.is_native() && !options.native_mod_adapter_sources;
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
    // The program entry function (`Main`) of an App or Test target is invoked by the host, not
    // by a source reference; the unused-private-item rule exempts it in the entry unit only.
    let entry_function = (plan.target.kind != TargetKind::Lib && !matches!(assembly.root_set, AssemblyRootSet::OwnUnits(_)))
        .then(|| crate::projects::PROGRAM_ENTRY_FUNCTION.to_owned());

    if use_session_cache && let Some(cached) = cached_executable_if_valid(&session_fingerprint, assembly.generation) {
        let syntax_generation_id = assembly.generation.0;
        Span::current().record("syntax_generation_id", syntax_generation_id);
        let front = cached.as_ref();
        let findings = fact_authority.fact_findings(&front.assembly, &front.program, pipeline)?;
        let fact_diagnostics = fact_findings_to_diagnostics(&front.assembly, entry_source, findings)?;
        if !collect_diagnostics {
            require_no_semantic_errors(&fact_diagnostics)?;
        }
        // A cached executable proves only that the entry unit was prepared; the structural rules
        // of every other own unit run again. The query gate above has already judged every own
        // unit, so its findings merge behind the rule diagnostics exactly as on the uncached path.
        let roots = own_unit_rule_diagnostics(&front.assembly, options, run_rules)?;
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
                executable: cached,
            },
            collected_diagnostics: diagnostics,
            collected_fixes: Vec::new(),
        });
    }

    let entry_unit = assembly.entry_unit().clone();
    let mut program = entry_unit.program.clone();
    observe_phase(pipeline, PARSE, || {});

    // The Mod host sees only the Mods this assembly activates (`mod_host::activation`): a Mod of a
    // package the program never reaches is not discovered, loaded or run.
    let mod_plan = crate::mod_host::active_mod_plan(plan, &assembly)?;
    let mod_input = ModHostInput {
        semantic_scope: fact_authority.mod_scope(),
        semantic_authority: None,
        compile_plan: Some(&mod_plan),
        source_name: &entry_unit.logical_name,
        source: entry_source,
        pipeline,
        mod_host: &options.mod_host,
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

    // The Mod scope already retains the metadata its authority issued (it carries the
    // attribution of appended contributions into the registered generation); a scope-less
    // prepare retains it here. Equal carriers are kept once.
    for metadata in generated.generator_outcomes.iter().flat_map(|outcome| outcome.compiled_metadata.iter()) {
        if !assembly.compiled_mod_metadata.contains(metadata) {
            assembly.compiled_mod_metadata.push(metadata.clone());
        }
    }
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
    rule_options.program_entry_function = entry_function;

    // Mod Analyzers read a semantic snapshot staged through composition, and their `Error`
    // diagnostics fail the executable path too. An executable-only prepare therefore stages the
    // snapshot itself when an Analyzer is registered and the session holds none for this exact
    // syntax generation; it never hands an Analyzer a snapshot of an older generation.
    let analyzers_registered =
        generated.session.registrations().any(|registration| registration.contract_id.ends_with(".Analyzer"));
    let current_snapshot_cached = use_session_cache
        && super::super::session::cached_semantic_snapshot(&session_fingerprint).is_some_and(|snapshot| {
            snapshot.syntax_generation_id == syntax_generation_id && snapshot.satisfies_minimum("composition")
        });
    let run_semantic_stage = options.front_end.with_semantic_diagnostics
        || collect_diagnostics
        || (analyzers_registered && !current_snapshot_cached);

    if run_semantic_stage {
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
                update_semantic_snapshot(&session_fingerprint, snapshot.clone());
            }
            local_semantic_snapshot = Some(snapshot);
        });
    }

    let mut composition_result = observe_phase_result(pipeline, COMPOSITION_RESOLVE, || {
        Ok::<_, anyhow::Error>(resolve_program_composition(&program, Some(plan)))
    })?;
    composition_result.snapshot.source_unit_path = Some(entry_unit.path.clone());

    if let Some(snapshot) = local_semantic_snapshot.take() {
        let snapshot = snapshot.with_composition(&composition_result.snapshot);
        if use_session_cache {
            update_semantic_snapshot(&session_fingerprint, snapshot.clone());
        }
        local_semantic_snapshot = Some(snapshot);
    }

    // The snapshot staged by this run is authoritative for its Analyzers; a session-cached one is
    // used only when it belongs to this exact syntax generation.
    let mod_snapshot = local_semantic_snapshot.clone().or_else(|| {
        use_session_cache
            .then(|| super::super::session::cached_semantic_snapshot(&session_fingerprint))
            .flatten()
            .filter(|snapshot| snapshot.syntax_generation_id == syntax_generation_id)
    });
    let mod_rewrite = run_analyze_rewrite_with_host(
        program.clone(),
        &generated.session,
        &options.mod_host,
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

    // The query gate: typing and legality obligations of every own unit (plus the direct-call
    // closure of each own item, possibly in dependency units) over the final rewritten syntax.
    // On the diagnostics path its findings are merged after the structural rules have run, so an
    // entry-unit error both report is reported once. On the executable path an error finding
    // fails preparation here: nothing after this point judges the program again.
    let findings = fact_authority.fact_findings(&assembly, &program, pipeline)?;
    let fact_diagnostics = fact_findings_to_diagnostics(&assembly, entry_source, findings)?;
    if !collect_diagnostics {
        require_no_semantic_errors(&fact_diagnostics)?;
    }

    // Mod analyzer diagnostics are always collected so a mod `Error`-severity
    // diagnostic fails the executable path (the same gate as compiler semantic errors). On the
    // diagnostics path they surface to the IDE; on the executable path `Warning`/`Note` are
    // dropped, matching the existing compiler-diagnostic contract.
    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
    // Mod quick-fixes are an IDE concern: collected only on the diagnostics path.
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

    // Every other own unit of the project runs the structural rules too, before the entry's
    // executable is cached, so a rule failure in a sibling unit can never hide behind a cached
    // entry. Their typing is judged by the query gate above.
    let root_diagnostics = own_unit_rule_diagnostics(&assembly, options, run_rules)?;
    if collect_diagnostics {
        collected_diagnostics.extend(root_diagnostics);
    } else {
        require_no_semantic_errors(&root_diagnostics)?;
    }

    observe_phase(pipeline, LOWER_READY, || {});

    // The executable front end is the rewritten syntax assembly with its composition authority.
    // Codegen builds its `TypedProgram` from it and reads every semantic fact from the same
    // generation-bound queries the gate judged; the legality gate runs again over the reachable
    // items at emission (`beskid_queries::check_items`).
    let executable = observe_phase_result(pipeline, LOWER, || {
        let executable = ExecutableFrontEnd {
            assembly: assembly.clone(),
            program: program.clone(),
            binding_plan: binding_plan.clone(),
            composition_snapshot: composition_snapshot.clone(),
        };
        let executable_snapshot = (if use_session_cache {
            super::super::session::cached_semantic_snapshot(&session_fingerprint)
        } else {
            local_semantic_snapshot.clone()
        })
        .map(SemanticSnapshot::with_executable)
        .unwrap_or_else(|| {
            SemanticSnapshot::from_diagnostics(&[], syntax_generation_id, "executable")
                .with_composition(&composition_snapshot)
                .with_executable()
        });
        if use_session_cache {
            store_executable_and_snapshot(&session_fingerprint, Some(executable), executable_snapshot)
                .ok_or_else(|| anyhow::anyhow!("entry session missing or changed before executable cache store"))
        } else {
            Ok(Arc::new(executable))
        }
    })?;

    merge_fact_diagnostics(&mut collected_diagnostics, fact_diagnostics);
    Ok(PrepareSpineOutput {
        prepared: PreparedCompilation { assembly, program, binding_plan, composition_snapshot, executable },
        collected_diagnostics: dedupe_diagnostics(collected_diagnostics),
        collected_fixes: analyzer_fixes,
    })
}

/// Structural and style rule diagnostics (`SemanticPipelineRule`) for every own unit of the
/// project other than the entry unit: the other roots of an entry-less library, or the sibling
/// units under an App's own source root ([`ProgramAssembly::library_unit_indices`]). Each unit is
/// judged as its own entry against the same assembly, so its diagnostics render in its own
/// source. Typing and legality of these units are the query gate's obligations, never judged
/// here. Fails closed when the root set names a unit outside the assembly.
fn own_unit_rule_diagnostics(
    assembly: &ProgramAssembly,
    options: &PrepareOptions,
    run_rules: bool,
) -> Result<Vec<SemanticDiagnostic>> {
    if !run_rules {
        return Ok(Vec::new());
    }
    let units = assembly
        .library_unit_indices()
        .ok_or_else(|| anyhow::anyhow!("assembly root set names a source unit outside the prepared assembly"))?;
    let known_module_paths = assembly.module_index.known_module_path_strings();
    let mut diagnostics = Vec::new();
    for index in units.into_iter().filter(|index| *index != assembly.entry_index) {
        let root_assembly = assembly
            .with_entry_index(index)
            .ok_or_else(|| anyhow::anyhow!("assembly root unit {index} is outside the prepared assembly"))?;
        let unit = &assembly.units[index];
        let mut rule_options = AnalysisOptions::default();
        rule_options.module_level_meta_items_allowed = options.front_end.module_level_meta_items_allowed;
        rule_options.known_assembly_module_paths = Some(known_module_paths.clone());
        rule_options.program_assembly_module_index = Some((*assembly.module_index).clone());
        rule_options.entry_source_path = Some(unit.path.clone());
        rule_options.program_assembly = Some(root_assembly);
        diagnostics.extend(semantic_rule_diagnostics_for_program_with_pipeline(
            &unit.program.node,
            unit.logical_name.clone(),
            &unit.source,
            rule_options,
            None,
        ));
    }
    Ok(diagnostics)
}
