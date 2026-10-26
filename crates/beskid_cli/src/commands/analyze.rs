//! `beskid check` — run builtin semantic rules and print diagnostics.

use anyhow::Result;
use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions};
use clap::Args;
use std::path::{Path, PathBuf};

use crate::machine_output::Records;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_tools::pipeline::{
    CliResolveOptions, resolve_input_with_cli_pipeline, tui::format_severity_summary, tui::severity_command_summary,
};

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    #[command(flatten)]
    pub diagnostics: crate::cli::error_format::ErrorFormatArgs,

    /// The input Beskid file to analyze
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Disable animated progress (resolve phases still emit pipeline events for tracing)
    #[arg(long)]
    pub plain: bool,

    /// Print schema-1 newline-delimited JSON records (`diagnostic`, then one `result`) on stdout
    #[arg(long)]
    pub json: bool,

    /// Check every member of the `.bws` workspace named by `--project`, each through all of its
    /// targets, in one process (members share the toolchain's Corelib and its analysis)
    #[arg(long = "all-members")]
    pub all_members: bool,
}

/// Resolve the project, analyze the entry source, and print diagnostics (or "No diagnostics.").
///
/// A project without an explicit `--target` is checked through every target of its manifest (App, Lib and
/// Test), so sources reachable only from a non-default target (per-suite test entries) are never skipped.
pub fn execute(args: AnalyzeArgs) -> Result<()> {
    if args.all_members {
        if args.input.is_some() || args.project.target.is_some() || args.project.workspace_member.is_some() {
            anyhow::bail!(
                "`--all-members` checks a whole workspace; it takes no input file, `--target` or `--workspace-member`"
            );
        }
        let project = args
            .project
            .project
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("`--all-members` needs `--project <workspace.bws or its directory>`"))?;
        let members = workspace_member_manifests(project)?;
        if args.json {
            return Records::new("check").run(|records| run_members(&args, &members, Some(records)));
        }
        return run_members(&args, &members, None).map(|_| ());
    }
    if let Some(manifest_path) = bsol_project_manifest(&args) {
        return check_bsol_project(&args, &manifest_path);
    }
    let targets = manifest_targets_to_check(&args);
    let project = args.project.project.as_deref();
    if args.json {
        return Records::new("check").run(|records| run_targets(&args, project, &targets, Some(records)));
    }
    run_targets(&args, project, &targets, None).map(|_| ())
}

/// The project manifest of every member of the workspace at `project` (a `.bws` file or the
/// directory that holds one), in manifest order.
fn workspace_member_manifests(project: &Path) -> Result<Vec<(String, PathBuf)>> {
    use beskid_analysis::projects::{
        discover_project_manifest_in_dir, discover_workspace_manifest_in_dir, parse_workspace_manifest,
    };
    let workspace = if project.is_dir() {
        discover_workspace_manifest_in_dir(project)?
            .ok_or_else(|| anyhow::anyhow!("no `.bws` workspace manifest in {}", project.display()))?
    } else if project.extension().is_some_and(|extension| extension == "bws") {
        project.to_path_buf()
    } else {
        anyhow::bail!("`--all-members` needs a `.bws` workspace, got {}", project.display());
    };
    let source = std::fs::read_to_string(&workspace)
        .map_err(|error| anyhow::anyhow!("read workspace manifest {}: {error}", workspace.display()))?;
    let manifest = parse_workspace_manifest(&source)?;
    let root = workspace.parent().unwrap_or_else(|| Path::new("."));
    manifest
        .members
        .iter()
        .map(|member| {
            let directory = root.join(&member.path);
            let manifest = discover_project_manifest_in_dir(&directory)?.ok_or_else(|| {
                anyhow::anyhow!("workspace member `{}` has no project manifest in {}", member.name, directory.display())
            })?;
            Ok((member.name.clone(), manifest))
        })
        .collect()
}

/// Check every member in this process. A failing member does not stop the others; the command
/// fails when any member failed.
fn run_members(
    args: &AnalyzeArgs,
    members: &[(String, PathBuf)],
    records: Option<&Records>,
) -> Result<serde_json::Value> {
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for (name, manifest) in members {
        let outcome = if is_bsol_manifest(manifest) {
            validate_bsol_manifest(args, manifest)
        } else {
            run_targets(args, Some(manifest), &targets_of_manifest(manifest), records)
        };
        match outcome {
            Ok(value) => results.push(serde_json::json!({ "member": name, "result": value })),
            Err(error) => failures.push(format!("member `{name}`: {error:#}")),
        }
    }
    if !failures.is_empty() {
        anyhow::bail!(
            "{} of {} workspace members failed analysis: {}",
            failures.len(),
            members.len(),
            failures.join("; ")
        );
    }
    Ok(serde_json::json!({ "members": results }))
}

/// The manifest `check` was pointed at, when it is one project manifest (a `.bproj` path or a
/// directory holding exactly one) and no input file, target or workspace member was selected.
fn selected_project_manifest(args: &AnalyzeArgs) -> Option<PathBuf> {
    if args.input.is_some() || args.project.target.is_some() || args.project.workspace_member.is_some() {
        return None;
    }
    let project = args.project.project.as_ref()?;
    if project.is_dir() {
        let entries = std::fs::read_dir(project).ok()?;
        let mut manifests = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "bproj"));
        match (manifests.next(), manifests.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    } else if project.extension().is_some_and(|extension| extension == "bproj") {
        Some(project.clone())
    } else {
        None
    }
}

/// The selected manifest when it declares a schema-only `type = Bsol` project.
fn bsol_project_manifest(args: &AnalyzeArgs) -> Option<PathBuf> {
    selected_project_manifest(args).filter(|manifest_path| is_bsol_manifest(manifest_path))
}

/// Whether the manifest declares a schema-only `type = Bsol` project.
fn is_bsol_manifest(manifest_path: &Path) -> bool {
    beskid_analysis::projects::load_manifest_from_path(manifest_path)
        .is_ok_and(|manifest| manifest.project.kind == beskid_analysis::projects::ProjectKind::Bsol)
}

/// A `type = Bsol` project has no Beskid source: checking it validates every schema export, the same
/// validation `beskid dev bsol validate <manifest>.bproj` runs.
fn check_bsol_project(args: &AnalyzeArgs, manifest_path: &Path) -> Result<()> {
    if args.json {
        return Records::new("check").run(|_records| validate_bsol_manifest(args, manifest_path));
    }
    validate_bsol_manifest(args, manifest_path).map(|_| ())
}

fn validate_bsol_manifest(args: &AnalyzeArgs, manifest_path: &Path) -> Result<serde_json::Value> {
    let manifest = beskid_analysis::projects::load_manifest_from_path(manifest_path)
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    let profiles = super::validate_bsol::validate_bsol_project(manifest_path, &manifest, None)?;
    if !args.json {
        for profile in &profiles {
            eprintln!("ok: validated exported schema profile `{profile}`");
        }
        eprintln!("Check complete: schema-only Bsol project, {} schema export(s) valid", profiles.len());
    }
    Ok(
        serde_json::json!({ "source": manifest_path, "bsol_profiles": profiles, "errors": 0, "warnings": 0, "notes": 0 }),
    )
}

/// Names of all manifest targets when `check` was pointed at one project manifest without a target selection and
/// that manifest declares several targets; empty means "check the one default target" as before.
fn manifest_targets_to_check(args: &AnalyzeArgs) -> Vec<String> {
    let Some(manifest_path) = selected_project_manifest(args) else { return Vec::new() };
    targets_of_manifest(&manifest_path)
}

/// Every target of a manifest that declares several; empty means its one default target.
fn targets_of_manifest(manifest_path: &Path) -> Vec<String> {
    match beskid_analysis::projects::load_manifest_from_path(manifest_path) {
        Ok(manifest) if manifest.targets.len() > 1 => {
            manifest.targets.iter().map(|target| target.name.clone()).collect()
        }
        _ => Vec::new(),
    }
}

fn run_targets(
    args: &AnalyzeArgs,
    project: Option<&Path>,
    targets: &[String],
    records: Option<&Records>,
) -> Result<serde_json::Value> {
    if targets.is_empty() {
        return run_analyze(args, project, args.project.target.as_deref(), records);
    }
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for target in targets {
        match run_analyze(args, project, Some(target), records) {
            Ok(value) => results.push(serde_json::json!({ "target": target, "result": value })),
            Err(error) => failures.push(format!("target `{target}`: {error:#}")),
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("{} of {} targets failed analysis: {}", failures.len(), targets.len(), failures.join("; "));
    }
    Ok(serde_json::json!({ "targets": results }))
}

/// Analyze one target. A failure that carries structured diagnostics (a parse error found while
/// the project is resolved or while its active Mods are decided) is reported like any other
/// diagnostic, with its code and at the authored file, so it is never flattened into a member's or
/// a target's failure line.
fn run_analyze(
    args: &AnalyzeArgs,
    project: Option<&Path>,
    target: Option<&str>,
    records: Option<&Records>,
) -> Result<serde_json::Value> {
    analyze_target(args, project, target, records).or_else(|error| {
        match services::structured_diagnostics(&error) {
            Some(diagnostics) if !diagnostics.is_empty() => report_failed_preparation(diagnostics, records),
            _ => Err(error),
        }
    })
}

fn report_failed_preparation(
    diagnostics: Vec<beskid_analysis::analysis::SemanticDiagnostic>,
    records: Option<&Records>,
) -> Result<serde_json::Value> {
    let diagnostics = beskid_tools::diagnostics::authored_diagnostics(diagnostics);
    for diagnostic in &diagnostics {
        beskid_tools::diagnostics::eprint_diagnostic(diagnostic);
        if let Some(records) = records {
            records.diagnostic(diagnostic)?;
        }
    }
    let counts = beskid_tools::pipeline::tui::count_severities(&diagnostics);
    anyhow::bail!("analysis failed with {}", format_severity_summary(counts))
}

fn analyze_target(
    args: &AnalyzeArgs,
    project: Option<&Path>,
    target: Option<&str>,
    records: Option<&Records>,
) -> Result<serde_json::Value> {
    let project = project.map(Path::to_path_buf);
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(CliResolveOptions::new(
        args.input.as_ref(),
        project.as_ref(),
        target,
        args.project.workspace_member.as_deref(),
        args.lockfile.workspace_options(),
        args.plain,
    ))?;
    let mod_host = super::compiler_mod::prepare_native_mod_executor(
        &resolved,
        args.lockfile.workspace_options(),
        Some(pipeline_ui.as_ref()),
    )?;
    let prepare_options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        ..PrepareOptions::with_mod_host(mod_host)
    };
    let diagnostics = if resolved.compile_plan.is_some() {
        let (_, diagnostics, _fixes) =
            beskid_queries::prepare_compilation_diagnostics(&resolved, prepare_options, Some(pipeline_ui.as_ref()))?;
        diagnostics
    } else if let Some(plan) = services::compile_plan_for_input_path(&resolved.source_path) {
        let project_resolved =
            services::resolved_input_from_plan(resolved.source_path.clone(), resolved.source.clone(), plan, None, None);
        let (_, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics(
            &project_resolved,
            prepare_options,
            Some(pipeline_ui.as_ref()),
        )?;
        diagnostics
    } else {
        beskid_tools::pipeline::frontend::standalone_semantic_diagnostics(
            &resolved.source_path,
            &resolved.source,
            Some(pipeline_ui.as_ref()),
        )?
    };
    let counts = pipeline_ui.report_semantic_diagnostics(&diagnostics);
    if let Some(records) = records {
        for diagnostic in &diagnostics {
            records.diagnostic(diagnostic)?;
        }
    }
    let severity_line = format_severity_summary(counts);
    let summary = severity_command_summary("Analyze", format!("Analyze complete ({severity_line})"), counts);
    pipeline_ui.finish_session_with_summary(format!("Analyze complete ({severity_line})"), Some(summary));
    if counts.errors != 0 {
        anyhow::bail!("analysis failed with {severity_line}");
    }
    Ok(serde_json::json!({
        "source": resolved.source_path,
        "errors": counts.errors,
        "warnings": counts.warnings,
        "notes": counts.notes,
    }))
}
