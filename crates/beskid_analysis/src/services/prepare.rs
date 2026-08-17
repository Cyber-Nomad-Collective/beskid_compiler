//! Unified compilation prepare spine consumed by analyze, run, build, test, and LSP.

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
use crate::analysis::rules::{RuleContext, resolve, types};
use crate::mod_host::diagnostics::analyzer_diagnostic_to_semantic;
use crate::mod_host::{
    ModHostInput, native_invoker_for_plan, run_analyze_rewrite_after_composition, run_through_generate,
};
use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProgramAssembly, SourceUnit, assemble_program, assembly_options_for_prepare,
};
use crate::syntax::Spanned;

use super::composition::{composition_result_to_diagnostics, resolve_program_composition};
use super::entry_session::{
    cached_executable_if_valid, current_syntax_generation_id, store_executable_and_snapshot, update_semantic_snapshot,
};
use super::front_end::{FrontEndOptions, FrontEndTypedResult};
use super::input::ResolvedInput;
use super::semantic::{require_no_semantic_errors, semantic_rule_diagnostics_for_program_with_pipeline};
use super::semantic_facts::{DependencyTypingPolicy, SemanticFactsError, resolve_and_type_program_with_assembly};
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
        Self { front_end: FrontEndOptions::default(), dependency_typing: DependencyTypingPolicy::FullClosure }
    }
}

/// Result of the unified prepare spine (typed syntax when lower succeeds).
pub struct PreparedCompilation {
    pub assembly: ProgramAssembly,
    pub program: Spanned<crate::syntax::Program>,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
    pub typed: Option<Arc<FrontEndTypedResult>>,
}

impl PreparedCompilation {
    /// Typed syntax bundle for codegen.
    pub fn into_executable(self) -> Result<FrontEndTypedResult> {
        let Some(typed) = self.typed else {
            return Err(anyhow::anyhow!(
                "prepare_compilation did not produce typed syntax (lower failed during diagnostic collection)"
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
        self.typed.as_ref().map(|typed| typed.as_ref()).ok_or_else(|| {
            anyhow::anyhow!(
                "prepare_compilation did not produce typed syntax (lower failed during diagnostic collection)"
            )
        })
    }

    /// Syntax-only project assembly for generation-safe consumers (LSP, queries, ISLE).
    ///
    /// Prefer this over reading [`ProgramAssembly::units`]. When typed syntax exists, the
    /// post-mod-rewrite entry program is projected; otherwise the prepare-spine rewritten
    /// `program` replaces the entry unit. Callers must not fall back to
    /// `DocumentAnalysisSnapshot` for IDE authority.
    pub fn syntax_assembly(&self) -> crate::projects::ProgramAssembly {
        if let Some(typed) = self.typed.as_ref() {
            return typed.syntax_assembly();
        }
        let mut units = self.assembly.units.as_ref().clone();
        units[self.assembly.entry_index].program = self.program.clone();
        crate::projects::ProgramAssembly::new(
            self.assembly.roots.clone(),
            Arc::new(units),
            self.assembly.entry_index,
            self.assembly.discovery,
            Arc::clone(&self.assembly.module_index),
            self.assembly.has_std_dependency,
            self.assembly.generation,
        )
        .with_trusted_corelib_service_paths(Arc::clone(&self.assembly.trusted_corelib_service_paths))
    }
}

/// Single front-end spine: assemble → mod host → rewrite → semantic → composition → (optional) typed syntax.
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

/// Like [`prepare_compilation`], collecting diagnostics from semantic, composition, and lower
/// instead of failing on errors.
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

fn session_fingerprint_field(fingerprint: &SessionFingerprint) -> String {
    format!("{}:{}", fingerprint.entry_canonical.display(), fingerprint.lockfile_digest)
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
            let assembled =
                assemble_program(plan, prepared_workspace, entry_path, Some(entry_source), &assembly_options, pipeline)
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
    rule_options.module_level_meta_items_allowed = options.front_end.module_level_meta_items_allowed;
    rule_options.known_assembly_module_paths = Some(assembly.module_index.known_module_path_strings());
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
                SemanticSnapshot::from_diagnostics(snapshot_diagnostics, syntax_generation_id, "semantic"),
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

    // Mod analyzer diagnostics are always collected so a mod `Error`-severity
    // diagnostic fails the typed/codegen build (mirrors `require_no_semantic_errors`
    // at the compiler-semantic gate above). On the diagnostics path they surface to
    // LSP; on the typed/codegen path `Warning`/`Note` are dropped, matching the
    // existing compiler-diagnostic contract.
    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, entry_unit, entry_source);
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
            let executable_snapshot = super::session::cached_semantic_snapshot(&session_fingerprint)
                .map(|snap| snap.with_typed_resolution(resolution_fingerprint, types_fingerprint))
                .unwrap_or_else(|| {
                    SemanticSnapshot::from_diagnostics(&[], syntax_generation_id, "executable")
                        .with_composition(&composition_snapshot)
                        .with_typed_resolution(resolution_fingerprint, types_fingerprint)
                });
            let stored = store_executable_and_snapshot(&session_fingerprint, Some(typed_result), executable_snapshot)
                .ok_or_else(|| anyhow::anyhow!("entry session missing for executable cache store"))?;
            Some(stored)
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

/// Map mod `Analyzer` outcomes into the semantic diagnostic stream.
///
/// Each `AnalyzerDiagnostic` is bridged via [`analyzer_diagnostic_to_semantic`], which tags
/// the diagnostic with `origin: Some("beskid:mod:<type_id>")` so LSP can route code actions
/// and the prepare spine can attribute build failures to mod contracts.
fn collect_analyzer_diagnostics(
    analyzer_outcomes: &[crate::mod_host::AnalyzerOutcome],
    entry_unit: &SourceUnit,
    entry_source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut out = Vec::new();
    for outcome in analyzer_outcomes {
        for diagnostic in &outcome.diagnostics {
            out.push(analyzer_diagnostic_to_semantic(
                diagnostic,
                outcome.type_id.as_str(),
                entry_unit.logical_name.as_str(),
                entry_source,
            ));
        }
    }
    out
}

fn semantic_facts_errors_to_diagnostics(
    error: SemanticFactsError,
    source_name: &str,
    source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut ctx = RuleContext::new(source_name, source, AnalysisOptions::default());
    match error {
        SemanticFactsError::Type { errors, typed } => {
            for error in errors {
                types::emit_type_error(&mut ctx, error, Some(&typed));
            }
        }
        SemanticFactsError::Resolve(errors) => {
            for error in errors {
                resolve::emit_resolve_error(&mut ctx, error);
            }
        }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{PrepareOptions, collect_analyzer_diagnostics, prepare_compilation, prepare_compilation_diagnostics};
    use crate::analysis::diagnostics::Severity;
    use crate::mod_host::{
        AnalyzerDiagnostic, AnalyzerSeverity, ContractInvoker, ContractRegistration, ModHostAnalyzeResult,
        ModInvocationContext, ScriptedContractInvoker,
    };
    use crate::projects::SourceUnit;
    use crate::services::semantic::require_no_semantic_errors;
    use crate::services::{
        FrontEndOptions, parse_program_with_source_name, resolved_input_from_plan, synthetic_compile_plan_for_source,
    };

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn prepare_spine_syntax_assembly_uses_rewritten_entry_without_document_snapshot() {
        let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("beskid_prepare_syntax_assembly_{test_id}"));
        std::fs::create_dir_all(&root).expect("test source root");
        let entry_path = root.join("Main.bd");
        let source = "i32 Main() { return 0; }";
        std::fs::write(&entry_path, source).expect("entry source");

        let plan = synthetic_compile_plan_for_source(&entry_path);
        let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
        let prepared = prepare_compilation(
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                ..Default::default()
            },
            None,
        )
        .expect("prepare");

        // Typed prepare projects through FrontEndTypedResult::syntax_assembly (post-rewrite entry).
        let syntax = prepared.syntax_assembly();
        assert_eq!(
            syntax.entry_unit().program,
            prepared.program,
            "prepare-spine syntax assembly must match the prepare entry program"
        );
        let typed = prepared.typed.as_ref().expect("typed front-end");
        assert_eq!(syntax.entry_unit().program, typed.syntax_assembly().entry_unit().program);

        // Untyped path: rewritten prepare.program is the sole authority (no DocumentAnalysisSnapshot).
        let rewritten = parse_program_with_source_name("Main.bd", "i32 Rewritten() { return 1; }").expect("rewritten");
        let mut untyped = prepared;
        untyped.typed = None;
        untyped.program = rewritten.clone();
        assert_eq!(
            untyped.syntax_assembly().entry_unit().program,
            rewritten,
            "untyped prepare-spine syntax assembly must project prepare.program"
        );
        assert!(!untyped.syntax_assembly().units.is_empty(), "syntax assembly must retain immutable source units");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_diagnostics_collect_without_legacy_document_snapshot() {
        let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("beskid_prepare_diags_no_snapshot_{test_id}"));
        std::fs::create_dir_all(&root).expect("test source root");
        let entry_path = root.join("Main.bd");
        // Intentional unresolved name so prepare-spine emits a diagnostic.
        let source = "i32 Main() { return Missing; }";
        std::fs::write(&entry_path, source).expect("entry source");

        let plan = synthetic_compile_plan_for_source(&entry_path);
        let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
        let (prepared, diags) = prepare_compilation_diagnostics(
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                ..Default::default()
            },
            None,
        )
        .expect("prepare diagnostics");

        let syntax = prepared.syntax_assembly();
        let expected = entry_path.canonicalize().unwrap_or(entry_path.clone());
        let actual = syntax.entry_unit().path.canonicalize().unwrap_or_else(|_| syntax.entry_unit().path.clone());
        assert_eq!(actual, expected);
        assert!(!diags.is_empty(), "prepare-spine diagnostics must surface without DocumentAnalysisSnapshot");

        let _ = std::fs::remove_dir_all(root);
    }

    /// Build a `ModHostAnalyzeResult` carrying one scripted `Analyzer` outcome so the
    /// prepare-spine mod-diagnostic gate can be exercised without a native mod artifact.
    fn mod_rewrite_with_scripted_analyzer(
        type_id: &str,
        diagnostic: AnalyzerDiagnostic,
        entry_source: &str,
    ) -> (ModHostAnalyzeResult, SourceUnit) {
        let invoker = ScriptedContractInvoker::new().with_analyzer_diagnostic(type_id, vec![diagnostic]);
        let registration = ContractRegistration {
            contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
            type_id: type_id.to_owned(),
            entry_symbol: "moda_check".to_owned(),
        };
        let context = ModInvocationContext::empty();
        let outcome = invoker
            .invoke_analyzer(&registration, &context.collect_request, None)
            .expect("scripted analyzer invocation");
        assert_eq!(outcome.diagnostics.len(), 1, "scripted analyzer diagnostic must overlay");

        let program = parse_program_with_source_name("Main.bd", entry_source).expect("parse entry");
        let entry_unit = SourceUnit {
            logical_name: "Main.bd".to_owned(),
            path: std::path::PathBuf::from("/tmp/Main.bd"),
            source: entry_source.to_owned(),
            program,
        };
        let mod_rewrite = ModHostAnalyzeResult {
            program: entry_unit.program.clone(),
            analyzer_outcomes: vec![outcome],
            rewriter_outcomes: Vec::new(),
            edited_source: None,
        };
        (mod_rewrite, entry_unit)
    }

    /// A mod `Error`-severity diagnostic must fail the typed/codegen build path
    /// (`require_no_semantic_errors` is the same gate used for compiler semantic errors).
    #[test]
    fn mod_error_diagnostic_fails_typed_build_path() {
        let entry_source = "unit Main() { return; }\n";
        let diagnostic = AnalyzerDiagnostic {
            code: "MOD0001".to_owned(),
            message: "mod error".to_owned(),
            severity: AnalyzerSeverity::Error,
            span: Some((0, 4)),
        };
        let (mod_rewrite, entry_unit) = mod_rewrite_with_scripted_analyzer("ModA.Check", diagnostic, entry_source);

        let analyzer_diagnostics =
            collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
        assert_eq!(analyzer_diagnostics.len(), 1);
        assert_eq!(analyzer_diagnostics[0].severity, Severity::Error);
        assert_eq!(analyzer_diagnostics[0].origin.as_deref(), Some("beskid:mod:ModA.Check"));

        // Typed/codegen path gate: mod Error fails the build.
        assert!(
            require_no_semantic_errors(&analyzer_diagnostics).is_err(),
            "mod Error-severity diagnostic must fail the typed build path"
        );
    }

    /// A mod `Warning`-severity diagnostic must NOT fail the typed/codegen build path
    /// (Warnings/Notes are dropped on the build path, matching the compiler-diagnostic contract).
    #[test]
    fn mod_warning_diagnostic_does_not_fail_typed_build_path() {
        let entry_source = "unit Main() { return; }\n";
        let diagnostic = AnalyzerDiagnostic {
            code: "MOD0002".to_owned(),
            message: "mod warning".to_owned(),
            severity: AnalyzerSeverity::Warning,
            span: Some((0, 4)),
        };
        let (mod_rewrite, entry_unit) = mod_rewrite_with_scripted_analyzer("ModA.Check", diagnostic, entry_source);

        let analyzer_diagnostics =
            collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
        assert_eq!(analyzer_diagnostics.len(), 1);
        assert_eq!(analyzer_diagnostics[0].severity, Severity::Warning);
        assert_eq!(analyzer_diagnostics[0].origin.as_deref(), Some("beskid:mod:ModA.Check"));

        // Typed/codegen path gate: mod Warning does not fail the build.
        assert!(
            require_no_semantic_errors(&analyzer_diagnostics).is_ok(),
            "mod Warning-severity diagnostic must not fail the typed build path"
        );
    }
}
