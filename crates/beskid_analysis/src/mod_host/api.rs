use anyhow::Result;

use crate::macros::run_macro_expand_with_diagnostics;
use crate::syntax::{Program, Spanned};

use super::capabilities::enforce_capabilities;
use super::collect::collect_contracts;
use super::discovery::discover_mod_dependencies;
use super::generate::run_generators;
use super::load::load_artifacts;
use super::merge::merge_generated_syntax;
use super::reparse::reparse_if_needed;
use super::rewrite::run_rewriters;
use super::types::{ModHostGenerateResult, ModHostInput, ModHostSession};

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
        });
    }

    let workspace_root = input.compile_plan.map(|plan| plan.project_root.as_path());
    let loaded = load_artifacts(workspace_root, discovered, input.pipeline)?;
    enforce_capabilities(&loaded)?;

    let collected = collect_contracts(&loaded, input.pipeline)?;
    let generated = run_generators(&loaded, &collected, input.pipeline)?;
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
    })
}

pub fn run_analyze_rewrite(
    program: Spanned<Program>,
    session: &ModHostSession,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
) -> Result<Spanned<Program>> {
    if session.is_empty() {
        return Ok(program);
    }

    let _analyzer_registrations = super::analyze::run_analyzers(session, pipeline)?;
    run_rewriters(program, session, pipeline)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use beskid_pipeline::{PipelineEvent, PipelineObserver};

    use crate::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};
    use crate::services::parse_program_with_source_name;

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
            },
        )
        .expect("mod host");

        assert!(result.session.is_empty());
        assert_eq!(
            pipeline.events.lock().expect("events").as_slice(),
            &[beskid_pipeline::phases::MACRO_EXPAND],
            "mod host should skip mod.* phases when the compile plan has no mod dependencies"
        );
    }

    #[test]
    fn emits_mod_phases_for_registered_generator_and_post_semantic_hooks() {
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
    capabilities = [emit_syntax, query_semantic_snapshot, rewrite_syntax]
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

        let generated = run_through_generate(
            program,
            &ModHostInput {
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source,
                pipeline: Some(pipeline.as_ref()),
            },
        )
        .expect("generate");
        let _program = run_analyze_rewrite(
            generated.program,
            &generated.session,
            Some(pipeline.as_ref()),
        )
        .expect("analyze rewrite");

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

        let _ = fs::remove_dir_all(root);
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
