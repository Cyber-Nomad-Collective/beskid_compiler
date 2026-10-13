use anyhow::Result;

use crate::macros::run_macro_expand_with_diagnostics;
use crate::syntax::{Program, Spanned};

use super::capabilities::enforce_capabilities;
use super::collect::{capture_target_fingerprint, collect_contracts, targets_changed};
use super::diagnostics::{ModHostDiagnostics, ModHostIssue};
use super::discovery::discover_mod_dependencies;
use super::generate::{is_generate_registration, resolved_max_generator_rounds, run_generators};
use super::generate_output::{load_generate_output_layout, write_typed_generate_output};
use super::invoker::ContractInvoker;
use super::invoker::GeneratorOutcome;
use super::host::ModHost;
use super::load::load_artifacts;
use super::merge::merge_generated_syntax;
use super::rewrite::run_rewriters;
use super::semantic_scope::invoke_in_scope;
use super::types::{
    LoadedModArtifact, ModHostAnalyzeResult, ModHostGenerateResult, ModHostInput, ModHostSession, ProgramItem,
};
use super::validate::validate_registrations;
use crate::projects::CompilePlan;
use crate::services::{SessionFingerprint, cached_semantic_snapshot};

/// Run `mod.collect` only and return the observed target fingerprint.
pub fn collect_mod_target_fingerprint(input: &ModHostInput<'_>) -> Result<String> {
    let discovered = discover_mod_dependencies(input.compile_plan)?;
    if discovered.is_empty() {
        return Ok(String::new());
    }
    let (invoker, runtime) = input.mod_host.require(&discovered)?;

    let workspace_root = input.compile_plan.map(|plan| plan.project_root.as_path());
    let loaded = load_artifacts(workspace_root, discovered, runtime, input.pipeline)?;
    enforce_capabilities(&loaded)?;
    if let Err(diagnostics) = validate_registrations(&loaded) {
        return Err(anyhow::Error::new(diagnostics));
    }

    let collected =
        invoke_in_scope(input, None, |scoped| collect_contracts(&loaded, scoped, invoker, scoped.pipeline))?.outcome;
    Ok(capture_target_fingerprint(&collected.outcomes))
}

pub fn run_through_generate(program: Spanned<Program>, input: &ModHostInput<'_>) -> Result<ModHostGenerateResult> {
    run_through_generate_with_output_policy(program, input, true)
}

/// Run collect/generate for an editor snapshot without writing declared outputs.
/// Generated syntax is still merged into the owned program used for diagnostics.
pub fn run_through_generate_without_materializing_outputs(
    program: Spanned<Program>,
    input: &ModHostInput<'_>,
) -> Result<ModHostGenerateResult> {
    run_through_generate_with_output_policy(program, input, false)
}

fn run_through_generate_with_output_policy(
    program: Spanned<Program>,
    input: &ModHostInput<'_>,
    materialize_outputs: bool,
) -> Result<ModHostGenerateResult> {
    let macro_outcome = run_macro_expand_with_diagnostics(program, input.pipeline, input.source_name, input.source)?;
    let macro_diagnostics = macro_outcome.diagnostics;
    let program = macro_outcome.program;
    let discovered = discover_mod_dependencies(input.compile_plan)?;
    if discovered.is_empty() {
        return Ok(ModHostGenerateResult {
            program,
            session: ModHostSession::default(),
            macro_diagnostics,
            collector_outcomes: Vec::new(),
            generator_outcomes: Vec::new(),
            target_fingerprint: String::new(),
            generators_skipped: false,
            generated_lineage: Vec::new(),
        });
    }

    let (invoker, runtime) = input.mod_host.require(&discovered)?;
    let workspace_root = input.compile_plan.map(|plan| plan.project_root.as_path());
    let loaded = load_artifacts(workspace_root, discovered, runtime, input.pipeline)?;
    schedule_generate(program, macro_diagnostics, input, invoker, loaded, materialize_outputs)
}

/// Everything after `mod.load`: static capability and registration validation (which must abort
/// before any scheduling), then the collect/generate rounds. Registrations come only from
/// qualified native descriptors, so validation cannot run before loading; it runs before collect.
fn schedule_generate(
    mut program: Spanned<Program>,
    mut macro_diagnostics: Vec<crate::analysis::SemanticDiagnostic>,
    input: &ModHostInput<'_>,
    invoker: &dyn ContractInvoker,
    loaded: Vec<LoadedModArtifact>,
    materialize_outputs: bool,
) -> Result<ModHostGenerateResult> {
    enforce_capabilities(&loaded)?;
    if let Err(diagnostics) = validate_registrations(&loaded) {
        return Err(anyhow::Error::new(diagnostics));
    }

    let max_rounds = resolved_max_generator_rounds(&loaded);
    let mut collector_outcomes = Vec::new();
    let mut generator_outcomes = Vec::new();
    let mut target_fingerprint = String::new();
    let mut generators_skipped = false;
    let mut generated_lineage = Vec::new();
    let mut generated;
    let mut round = 0u32;
    // Generators are incremental: a replay round runs only when the previous round's merged
    // syntax changed the collected target set. Identical targets mean the merged program already
    // holds every contribution for them, so the replay loop has reached its fixed point.
    let mut converged = false;
    let mut previous_round_fingerprint: Option<String> = None;
    while round < max_rounds {
        round += 1;
        let collection = invoke_in_scope(input, Some(&program), |scoped| {
            collect_contracts(&loaded, scoped, invoker, scoped.pipeline)
        })?;
        if let Some(assembly) = collection.assembly {
            program = assembly.entry_unit().program.clone();
        }
        let collected = collection.outcome;
        target_fingerprint = capture_target_fingerprint(&collected.outcomes);
        collector_outcomes = collected.outcomes.clone();

        if !targets_changed(input.cached_target_fingerprint, &target_fingerprint) {
            generators_skipped = true;
            converged = true;
            break;
        }
        if previous_round_fingerprint.as_deref() == Some(target_fingerprint.as_str()) {
            converged = true;
            break;
        }
        previous_round_fingerprint = Some(target_fingerprint.clone());

        let invocation = invoke_in_scope(input, Some(&program), |scoped| {
            run_generators(&loaded, &collected, scoped, invoker, scoped.pipeline)
        })?;
        let invocation_generation = invocation.issued_generation;
        if let Some(assembly) = invocation.assembly {
            program = assembly.entry_unit().program.clone();
        }
        generated = invocation.outcome;
        generator_outcomes = generated.outcomes.clone();
        if materialize_outputs {
            materialize_declared_outputs(input.compile_plan, &loaded, &generator_outcomes)?;
        }
        if !generated.has_typed_merge() {
            converged = true;
            break;
        }
        let merged = merge_generated_syntax(program, &generated, invocation_generation)?;
        program = merged.program;
        generated_lineage.extend(merged.lineage);
        let needs_another_round = collected.outcomes.iter().any(|outcome| !outcome.narrowed_targets.is_empty());
        if !needs_another_round {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(anyhow::Error::new(ModHostDiagnostics::new(vec![ModHostIssue::MaxGeneratorRoundsExceeded {
            limit: max_rounds,
        }])));
    }
    let macro_outcome = run_macro_expand_with_diagnostics(program, input.pipeline, input.source_name, input.source)?;
    program = macro_outcome.program;
    macro_diagnostics.extend(macro_outcome.diagnostics);

    Ok(ModHostGenerateResult {
        program,
        session: ModHostSession::new(loaded),
        macro_diagnostics,
        collector_outcomes,
        generator_outcomes,
        target_fingerprint,
        generators_skipped,
        generated_lineage,
    })
}

fn materialize_declared_outputs(
    compile_plan: Option<&CompilePlan>,
    loaded: &[LoadedModArtifact],
    outcomes: &[GeneratorOutcome],
) -> Result<()> {
    for artifact in loaded {
        let Some(outputs) = artifact
            .discovered
            .mod_section
            .as_ref()
            .and_then(|section| section.generated_outputs.as_ref())
            .filter(|outputs| !outputs.is_empty())
        else {
            continue;
        };

        let typed_items = typed_items_for_artifact(artifact, outcomes);
        if typed_items.is_empty() {
            continue;
        }
        for output in outputs {
            let layout_path = artifact.discovered.project_root.join(&output.layout);
            let layout = load_generate_output_layout(&layout_path).map_err(|err| anyhow::anyhow!("{err}"))?;
            let output_root = artifact.discovered.project_root.join(output.resolved_root());
            write_typed_generate_output(compile_plan, &artifact.discovered.project_root, &output_root, &typed_items, &layout)
                .map_err(|err| anyhow::anyhow!("{err}"))?;
        }
    }
    Ok(())
}

fn typed_items_for_artifact(artifact: &LoadedModArtifact, outcomes: &[GeneratorOutcome]) -> Vec<Spanned<ProgramItem>> {
    use std::collections::HashSet;

    let generator_type_ids: HashSet<&str> = artifact
        .registrations
        .iter()
        .filter(|registration| is_generate_registration(registration))
        .map(|registration| registration.type_id.as_str())
        .collect();

    outcomes
        .iter()
        .filter(|outcome| generator_type_ids.contains(outcome.type_id.as_str()))
        .flat_map(|outcome| outcome.typed_items.iter().cloned())
        .collect()
}

/// Run `mod.analyze` then `mod.rewrite` after semantic snapshot and composition (prepare spine).
pub fn run_analyze_rewrite_after_composition(
    program: Spanned<Program>,
    session: &ModHostSession,
    fingerprint: &SessionFingerprint,
    mod_host: &ModHost,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<ModHostAnalyzeResult> {
    run_analyze_rewrite_with_host(
        program,
        session,
        mod_host,
        None,
        cached_semantic_snapshot(fingerprint).as_ref(),
        pipeline,
    )
}

/// Run `mod.analyze` then `mod.rewrite` for the Mods loaded into `session`, dispatching through
/// `mod_host`, and expose per-contract outcomes for the engine and tests that assert which
/// Analyzer / Rewriter contracts ran.
pub fn run_analyze_rewrite_with_host(
    program: Spanned<Program>,
    session: &ModHostSession,
    mod_host: &ModHost,
    host_input: Option<&ModHostInput<'_>>,
    snapshot: Option<&crate::services::SemanticSnapshot>,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<ModHostAnalyzeResult> {
    if session.is_empty() {
        return Ok(ModHostAnalyzeResult {
            program,
            analyzer_outcomes: Vec::new(),
            rewriter_outcomes: Vec::new(),
            edited_source: None,
        });
    }
    let (invoker, _) =
        mod_host.require(session.loaded_artifacts().iter().map(|artifact| &artifact.discovered))?;
    analyze_rewrite_with_invoker(program, session, invoker, host_input, snapshot, pipeline)
}

/// `mod.analyze` and `mod.rewrite` for a non-empty `session` through an already selected invoker.
pub(super) fn analyze_rewrite_with_invoker(
    program: Spanned<Program>,
    session: &ModHostSession,
    invoker: &dyn ContractInvoker,
    host_input: Option<&ModHostInput<'_>>,
    snapshot: Option<&crate::services::SemanticSnapshot>,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<ModHostAnalyzeResult> {
    let mut program = program;
    let analyzed = if let Some(input) = host_input {
        let invocation = invoke_in_scope(input, Some(&program), |scoped| {
            super::analyze::run_analyzers(session, Some(scoped), invoker, snapshot, pipeline)
        })?;
        if let Some(assembly) = invocation.assembly {
            program = assembly.entry_unit().program.clone();
        }
        invocation.outcome
    } else {
        super::analyze::run_analyzers(session, None, invoker, snapshot, pipeline)?
    };
    let analyzer_outcomes = analyzed.outcomes.clone();
    // Source text for rewriter edit application comes from the host input when
    // available. Callers without a `ModHostInput` (e.g. the bare
    // `run_analyze_rewrite` entry point) pass `None`, which skips edit application
    // and preserves the previous record-only behavior.
    let source = host_input.map(|input| input.source);
    let rewrite = if let Some(input) = host_input {
        let invocation = invoke_in_scope(input, Some(&program), |scoped| {
            run_rewriters(program.clone(), source, session, &analyzed, Some(scoped), invoker, pipeline)
        })?;
        let mut rewrite = invocation.outcome;
        if let Some(assembly) = invocation.assembly {
            rewrite.program = assembly.entry_unit().program.clone();
        }
        rewrite
    } else {
        run_rewriters(program, source, session, &analyzed, None, invoker, pipeline)?
    };
    let program = super::glue::run_glue(rewrite.program, session, pipeline)?;

    Ok(ModHostAnalyzeResult {
        program,
        analyzer_outcomes,
        rewriter_outcomes: rewrite.outcomes,
        edited_source: rewrite.edited_source,
    })
}

/// Surface the structured [`ModHostDiagnostics`] from a `mod_host` error. Returns
/// `None` when the error originated outside the mod-host validation pass.
pub fn extract_mod_host_diagnostics(err: &anyhow::Error) -> Option<&ModHostDiagnostics> {
    err.downcast_ref::<ModHostDiagnostics>()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use beskid_pipeline::{PipelineEvent, PipelineObserver};

    use crate::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};
    use crate::services::parse_program_with_source_name;

    use super::super::invoker::{InvocationKind, StubContractInvoker};
    use super::super::types::{ContractRegistration, ModArtifactDescriptor};
    use super::*;

    pub(super) const HOST_MANIFEST: &str = r#"Host {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "ModA" {
  source = path
  path = "../ModA"
}
"#;

    pub(super) const MODA_MANIFEST: &str = r#"ModA {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot, rewrite_syntax]
  }
}
"#;

    #[derive(Default)]
    pub(super) struct CapturePipeline {
        pub(super) events: Mutex<Vec<&'static str>>,
    }

    impl PipelineObserver for CapturePipeline {
        fn on_event(&self, event: PipelineEvent) {
            if let PipelineEvent::PhaseStart { id } = event {
                self.events.lock().expect("events").push(id);
            }
        }
    }

    #[test]
    fn skips_all_mod_phases_when_plan_has_no_mod_dependencies() {
        let source = "unit Main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let pipeline = CapturePipeline::default();

        let result = run_through_generate(
            program,
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: None,
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                mod_host: &ModHost::NoMods,
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
        )
        .expect("mod host");

        assert!(result.session.is_empty());
        assert!(result.collector_outcomes.is_empty());
        assert!(result.generator_outcomes.is_empty());
        assert_eq!(
            pipeline.events.lock().expect("events").as_slice(),
            &[beskid_pipeline::phases::MACRO_EXPAND],
            "mod host should skip mod.* phases when the compile plan has no mod dependencies"
        );
    }

    /// Orchestration tests cannot run native Mod code: native execution requires a qualified
    /// installed prefix and a built Mod executable, which a unit-test binary under
    /// `target/*/deps` never has (that stays fail-closed, see `load.rs`). They therefore enter
    /// the scheduler after `mod.load` with already-loaded registrations and a stub invoker.
    pub(super) fn loaded_mod(
        host: &std::path::Path,
        mod_dir: &std::path::Path,
        registrations: &[(&str, &str, &str)],
    ) -> Vec<LoadedModArtifact> {
        let plan = compile_plan(host, mod_dir);
        let discovered = discover_mod_dependencies(Some(&plan)).expect("discover");
        let registrations: Vec<ContractRegistration> = registrations
            .iter()
            .map(|(contract, type_id, symbol)| ContractRegistration {
                contract_id: (*contract).to_owned(),
                type_id: (*type_id).to_owned(),
                entry_symbol: (*symbol).to_owned(),
            })
            .collect();
        discovered
            .into_iter()
            .map(|discovered| {
                let mut descriptor = ModArtifactDescriptor::context_fixture();
                descriptor.package_id = discovered.project_name.clone();
                descriptor.registrations = registrations.clone();
                LoadedModArtifact { discovered, descriptor: Some(descriptor), registrations: registrations.clone() }
            })
            .collect()
    }

    pub(super) fn write_mod_projects(root: &std::path::Path, mod_manifest: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let host = root.join("Host");
        let mod_dir = root.join("ModA");
        fs::create_dir_all(host.join("Src")).expect("host src");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod src");
        fs::write(host.join("Host.bproj"), HOST_MANIFEST).expect("host manifest");
        fs::write(mod_dir.join("ModA.bproj"), mod_manifest).expect("mod manifest");
        (host, mod_dir)
    }

    #[test]
    fn invokes_each_contract_kind_through_pipeline() {
        let root = unique_temp_dir("mod_host_pipeline");
        let (host, mod_dir) = write_mod_projects(&root, MODA_MANIFEST);
        let loaded = loaded_mod(
            &host,
            &mod_dir,
            &[
                ("Beskid.Compiler.Collect.Collector", "ModA.Collect", "moda_collect"),
                ("Beskid.Compiler.Collect.Generator", "ModA.Emit", "moda_emit"),
                ("Beskid.Compiler.Collect.Analyzer", "ModA.Check", "moda_check"),
                ("Beskid.Compiler.Collect.Rewriter", "ModA.Rewrite", "moda_rewrite"),
            ],
        );

        let source = "unit Main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = Arc::new(CapturePipeline::default());
        let invoker = StubContractInvoker::new();

        let generated = schedule_generate(
            program,
            Vec::new(),
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(pipeline.as_ref()),
                mod_host: &ModHost::NoMods,
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
            &invoker,
            loaded,
            true,
        )
        .expect("generate");
        assert_eq!(generated.collector_outcomes.len(), 1);
        assert_eq!(generated.generator_outcomes.len(), 1);

        let composition_snapshot = generated.session.composition_snapshot_or_default();
        let semantic_snapshot = crate::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
            .with_composition(&composition_snapshot);
        let analyze = analyze_rewrite_with_invoker(
            generated.program,
            &generated.session,
            &invoker,
            None,
            Some(&semantic_snapshot),
            Some(pipeline.as_ref()),
        )
        .expect("analyze rewrite");
        assert_eq!(analyze.analyzer_outcomes.len(), 1);
        assert_eq!(analyze.rewriter_outcomes.len(), 1);

        let invocations = invoker.invocations();
        assert_eq!(invocations.len(), 4, "all four contract kinds invoked");
        assert!(matches!(invocations[0], InvocationKind::Collector { .. }));
        assert!(matches!(invocations[1], InvocationKind::Generator { .. }));
        assert!(matches!(invocations[2], InvocationKind::Analyzer { .. }));
        assert!(matches!(invocations[3], InvocationKind::Rewriter { .. }));

        let events = pipeline.events.lock().expect("events").clone();
        assert_eq!(
            events,
            vec![
                beskid_pipeline::phases::MOD_COLLECT,
                beskid_pipeline::phases::MOD_GENERATE,
                beskid_pipeline::phases::MACRO_EXPAND,
                beskid_pipeline::phases::MOD_ANALYZE,
                beskid_pipeline::phases::MOD_REWRITE,
                beskid_pipeline::phases::MOD_GLUE,
            ]
        );

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn unqualified_test_binary_fails_closed_before_any_mod_scheduling() {
        let root = unique_temp_dir("mod_host_unqualified");
        let (host, mod_dir) = write_mod_projects(&root, MODA_MANIFEST);
        let descriptor_dir = host.join(".beskid/obj/mods/ModA/cache-key/test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        fs::write(descriptor_dir.join("mod.descriptor.json"), "{}").expect("descriptor");

        let source = "unit Main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = CapturePipeline::default();
        let invoker = Arc::new(StubContractInvoker::new());
        let mod_host = ModHost::native(
            invoker.clone(),
            super::super::host::RuntimeKitSelection::fixture(
                &std::env::temp_dir().join("beskid-test-runtime-prefix"),
                beskid_abi::runtime_kit::BuildProfile::Debug,
            ),
        );
        let error = match run_through_generate(
            program,
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                mod_host: &mod_host,
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
        ) {
            Ok(_) => panic!("an unqualified descriptor must not load"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("no current qualified native Mod artifact"), "{error}");
        assert!(invoker.invocations().is_empty());
        let events = pipeline.events.lock().expect("events").clone();
        assert!(!events.contains(&beskid_pipeline::phases::MOD_COLLECT));

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn no_mod_host_fails_before_mod_load_naming_the_mod() {
        let root = unique_temp_dir("mod_host_no_mods");
        let (host, mod_dir) = write_mod_projects(&root, MODA_MANIFEST);
        let source = "unit Main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = CapturePipeline::default();
        let error = match run_through_generate(
            program,
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                mod_host: &ModHost::NoMods,
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
        ) {
            Ok(_) => panic!("a Mod dependency cannot run without a Mod host"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("`ModA`") && message.contains("ModA.bproj"), "{message}");
        assert!(message.contains("ModHost::native"), "{message}");
        let events = pipeline.events.lock().expect("events").clone();
        assert!(!events.contains(&beskid_pipeline::phases::MOD_LOAD), "no artifact is loaded without a host");

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn duplicate_registration_aborts_before_collect_with_e1829() {
        let root = unique_temp_dir("mod_host_dup");
        let (host, mod_dir) = write_mod_projects(
            &root,
            r#"ModA {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}
"#,
        );
        let loaded = loaded_mod(
            &host,
            &mod_dir,
            &[
                ("Beskid.Compiler.Collect.Generator", "ModA.Emit", "sym1"),
                ("Beskid.Compiler.Collect.Generator", "ModA.Emit", "sym2"),
            ],
        );

        let source = "unit Main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = CapturePipeline::default();
        let invoker = StubContractInvoker::new();

        let result = schedule_generate(
            program,
            Vec::new(),
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                mod_host: &ModHost::NoMods,
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
            &invoker,
            loaded,
            true,
        );
        let err = match result {
            Ok(_) => panic!("duplicate (contractId, typeId) registration must abort"),
            Err(err) => err,
        };
        let diagnostics =
            extract_mod_host_diagnostics(&err).expect("mod host diagnostics surfaced through anyhow chain");
        assert!(diagnostics.codes().contains(&"E1829"));

        let events = pipeline.events.lock().expect("events").clone();
        assert!(!events.contains(&beskid_pipeline::phases::MOD_COLLECT), "scheduling must abort before mod.collect");
        assert!(invoker.invocations().is_empty());

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    pub(super) fn compile_plan(host: &std::path::Path, mod_dir: &std::path::Path) -> CompilePlan {
        CompilePlan {
            project_root: host.to_path_buf(),
            manifest_path: host.join("Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: host.join("Src"),
            target: Target { name: "main".to_owned(), kind: TargetKind::App, entry: Some("Main.bd".to_owned()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "ModA".to_owned(),
                manifest_path: mod_dir.join("ModA.bproj"),
                project_root: mod_dir.to_path_buf(),
                project_name: "ModA".to_owned(),
                source_root: mod_dir.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_core_dependency: false,
        }
    }

    pub(super) fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}

#[cfg(test)]
mod scheduling_tests;
