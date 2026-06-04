use anyhow::Result;

use crate::macros::run_macro_expand_with_diagnostics;
use crate::syntax::{Program, Spanned};

use super::capabilities::enforce_capabilities;
use super::collect::collect_contracts;
use super::diagnostics::ModHostDiagnostics;
use super::discovery::discover_mod_dependencies;
use super::generate::run_generators;
use super::invoker::{ContractInvoker, StubContractInvoker};
use super::load::load_artifacts;
use super::merge::merge_generated_syntax;
use super::reparse::reparse_if_needed;
use super::rewrite::run_rewriters;
use super::types::{ModHostAnalyzeResult, ModHostGenerateResult, ModHostInput, ModHostSession};
use crate::services::{SessionFingerprint, cached_semantic_snapshot};
use super::validate::validate_registrations;

pub fn run_through_generate(
    program: Spanned<Program>,
    input: &ModHostInput<'_>,
) -> Result<ModHostGenerateResult> {
    let macro_outcome = run_macro_expand_with_diagnostics(
        program,
        input.pipeline,
        input.source_name,
        input.source,
    )?;
    let mut macro_diagnostics = macro_outcome.diagnostics;
    let mut program = macro_outcome.program;
    let discovered = discover_mod_dependencies(input.compile_plan)?;
    if discovered.is_empty() {
        return Ok(ModHostGenerateResult {
            program,
            session: ModHostSession::default(),
            macro_diagnostics,
            collector_outcomes: Vec::new(),
            generator_outcomes: Vec::new(),
        });
    }

    let workspace_root = input.compile_plan.map(|plan| plan.project_root.as_path());
    let loaded = load_artifacts(workspace_root, discovered, input.pipeline)?;
    enforce_capabilities(&loaded)?;
    if let Err(diagnostics) = validate_registrations(&loaded) {
        return Err(anyhow::Error::new(diagnostics));
    }

    let default_invoker = StubContractInvoker::new();
    let invoker: &dyn ContractInvoker = match input.invoker {
        Some(invoker) => invoker,
        None => &default_invoker,
    };

    let collected = collect_contracts(&loaded, invoker, input.pipeline)?;
    let generated = run_generators(&loaded, &collected, invoker, input.pipeline)?;
    let collector_outcomes = collected.outcomes.clone();
    let generator_outcomes = generated.outcomes.clone();
    program = merge_generated_syntax(program, &generated)?;
    program = reparse_if_needed(
        program,
        &generated,
        input.source_name,
        input.source,
        input.pipeline,
    )?;
    let macro_outcome = run_macro_expand_with_diagnostics(
        program,
        input.pipeline,
        input.source_name,
        input.source,
    )?;
    program = macro_outcome.program;
    macro_diagnostics.extend(macro_outcome.diagnostics);

    Ok(ModHostGenerateResult {
        program,
        session: ModHostSession::new(loaded),
        macro_diagnostics,
        collector_outcomes,
        generator_outcomes,
    })
}

pub fn run_analyze_rewrite(
    program: Spanned<Program>,
    session: &ModHostSession,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<Spanned<Program>> {
    Ok(run_analyze_rewrite_with_invoker(program, session, None, None, pipeline)?.program)
}

/// Run `mod.analyze` then `mod.rewrite` after semantic snapshot and composition (prepare spine).
pub fn run_analyze_rewrite_after_composition(
    program: Spanned<Program>,
    session: &ModHostSession,
    fingerprint: &SessionFingerprint,
    invoker: Option<&dyn ContractInvoker>,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<ModHostAnalyzeResult> {
    run_analyze_rewrite_with_invoker(
        program,
        session,
        invoker,
        cached_semantic_snapshot(fingerprint).as_ref(),
        pipeline,
    )
}

/// Like [`run_analyze_rewrite`] but exposes per-contract outcomes for engine and tests
/// that need to assert which Analyzer / Rewriter contracts ran.
pub fn run_analyze_rewrite_with_invoker(
    program: Spanned<Program>,
    session: &ModHostSession,
    invoker: Option<&dyn ContractInvoker>,
    snapshot: Option<&crate::services::SemanticSnapshot>,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<ModHostAnalyzeResult> {
    if session.is_empty() {
        return Ok(ModHostAnalyzeResult {
            program,
            analyzer_outcomes: Vec::new(),
            rewriter_outcomes: Vec::new(),
        });
    }

    let default_invoker = StubContractInvoker::new();
    let invoker: &dyn ContractInvoker = match invoker {
        Some(invoker) => invoker,
        None => &default_invoker,
    };

    let analyzed = super::analyze::run_analyzers(session, invoker, snapshot, pipeline)?;
    let analyzer_outcomes = analyzed.outcomes.clone();
    let rewrite = run_rewriters(program, session, &analyzed, invoker, pipeline)?;

    Ok(ModHostAnalyzeResult {
        program: rewrite.program,
        analyzer_outcomes,
        rewriter_outcomes: rewrite.outcomes,
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
    use super::*;

    #[derive(Default)]
    struct CapturePipeline {
        events: Mutex<Vec<&'static str>>,
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
        let source = "unit main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let pipeline = CapturePipeline::default();

        let result = run_through_generate(
            program,
            &ModHostInput {
                compile_plan: None,
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                invoker: None,
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

    #[test]
    fn invokes_each_contract_kind_through_pipeline() {
        let root = unique_temp_dir("mod_host_pipeline");
        let host = root.join("Host");
        let mod_dir = root.join("ModA");
        fs::create_dir_all(host.join("Src")).expect("host src");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod src");
        fs::write(host.join("Project.proj"), "placeholder").expect("host manifest");
        fs::write(
            mod_dir.join("Project.proj"),
            r#"
project {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot, rewrite_syntax]
  }
}
"#,
        )
        .expect("mod manifest");
        let descriptor_dir = host.join(".beskid/obj/mods/ModA/cache-key/test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        fs::write(
            descriptor_dir.join("mod.descriptor.json"),
            r#"{
  "schemaVersion": 1,
  "packageId": "ModA",
  "modSourceHash": "source",
  "lockHash": "lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": [
    {
      "contractId": "Beskid.Compiler.Collect.Collector",
      "typeId": "ModA.Collect",
      "entrySymbol": "moda_collect"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Generator",
      "typeId": "ModA.Emit",
      "entrySymbol": "moda_emit"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Analyzer",
      "typeId": "ModA.Check",
      "entrySymbol": "moda_check"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Rewriter",
      "typeId": "ModA.Rewrite",
      "entrySymbol": "moda_rewrite"
    }
  ]
}"#,
        )
        .expect("descriptor");

        let source = "unit main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = Arc::new(CapturePipeline::default());
        let invoker = StubContractInvoker::new();

        let generated = run_through_generate(
            program,
            &ModHostInput {
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(pipeline.as_ref()),
                invoker: Some(&invoker),
            },
        )
        .expect("generate");
        assert_eq!(generated.collector_outcomes.len(), 1);
        assert_eq!(generated.generator_outcomes.len(), 1);

        let composition_snapshot = generated.session.composition_snapshot_or_default();
        let semantic_snapshot = crate::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
            .with_composition(&composition_snapshot);
        let analyze = run_analyze_rewrite_with_invoker(
            generated.program,
            &generated.session,
            Some(&invoker),
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
                beskid_pipeline::phases::MACRO_EXPAND,
                beskid_pipeline::phases::MOD_LOAD,
                beskid_pipeline::phases::MOD_COLLECT,
                beskid_pipeline::phases::MOD_GENERATE,
                beskid_pipeline::phases::SYNTAX_GENERATION,
                beskid_pipeline::phases::MACRO_EXPAND,
                beskid_pipeline::phases::MOD_ANALYZE,
                beskid_pipeline::phases::MOD_REWRITE,
            ]
        );

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn duplicate_registration_aborts_before_collect_with_e1829() {
        let root = unique_temp_dir("mod_host_dup");
        let host = root.join("Host");
        let mod_dir = root.join("ModA");
        fs::create_dir_all(host.join("Src")).expect("host src");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod src");
        fs::write(host.join("Project.proj"), "placeholder").expect("host manifest");
        fs::write(
            mod_dir.join("Project.proj"),
            r#"
project {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}
"#,
        )
        .expect("mod manifest");
        let descriptor_dir = host.join(".beskid/obj/mods/ModA/cache-key/test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        fs::write(
            descriptor_dir.join("mod.descriptor.json"),
            r#"{
  "schemaVersion": 1,
  "packageId": "ModA",
  "modSourceHash": "source",
  "lockHash": "lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": [
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "ModA.Emit", "entrySymbol": "sym1" },
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "ModA.Emit", "entrySymbol": "sym2" }
  ]
}"#,
        )
        .expect("descriptor");

        let source = "unit main() { return; }\n";
        let program = parse_program_with_source_name("Main.bd", source).expect("parse");
        let plan = compile_plan(&host, &mod_dir);
        let pipeline = CapturePipeline::default();

        let result = run_through_generate(
            program,
            &ModHostInput {
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(&pipeline),
                invoker: None,
            },
        );
        let err = match result {
            Ok(_) => panic!("duplicate (contractId, typeId) registration must abort"),
            Err(err) => err,
        };
        let diagnostics = extract_mod_host_diagnostics(&err)
            .expect("mod host diagnostics surfaced through anyhow chain");
        assert!(diagnostics.codes().contains(&"E1829"));

        let events = pipeline.events.lock().expect("events").clone();
        assert!(
            !events.contains(&beskid_pipeline::phases::MOD_COLLECT),
            "scheduling must abort before mod.collect"
        );

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    fn compile_plan(host: &std::path::Path, mod_dir: &std::path::Path) -> CompilePlan {
        CompilePlan {
            project_root: host.to_path_buf(),
            manifest_path: host.join("Project.proj"),
            project_name: "Host".to_owned(),
            source_root: host.join("Src"),
            target: Target {
                name: "main".to_owned(),
                kind: TargetKind::App,
                entry: "Main.bd".to_owned(),
            },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "ModA".to_owned(),
                manifest_path: mod_dir.join("Project.proj"),
                project_root: mod_dir.to_path_buf(),
                project_name: "ModA".to_owned(),
                source_root: mod_dir.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        }
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}
