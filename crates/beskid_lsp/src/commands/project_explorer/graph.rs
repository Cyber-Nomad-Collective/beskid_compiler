//! Project graph and dependency execute-command payloads.

use std::{
    collections::HashSet,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use beskid_analysis::{
    CompilationContext,
    projects::{
        CompilePlan, DependencySource, PortableLockPath, PortableLockPathBaseKind, ProjectLockDependencyEntry,
        ProjectLockSource, build_compile_plan, effective_roots_from_lockfile_checked, is_workspace_manifest_path,
        load_project_lock_dependencies_for_plan, parse_manifest, plan_entry_path,
    },
};
use beskid_graph::{GraphKind, graph_tooling_payload};
use beskid_queries::{GraphFetchRequest, get_graph_document, get_graph_document_simple};
use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::{Error, Result};

use crate::protocol::execute_args::missing_args;

pub(crate) fn get_graph(
    project_uri: &str,
    kind: GraphKind,
    entry_uri: Option<&str>,
    workspace_uri: Option<&str>,
    db: Option<&mut beskid_queries::BeskidDatabase>,
) -> Result<Value> {
    let manifest_path = super::manifest_path_from_uri(project_uri)?;
    let workspace_manifest = resolve_workspace_manifest(&manifest_path, kind, workspace_uri)?;
    let mut entry_path = entry_uri.and_then(|uri| {
        crate::workspace_scan::path_from_uri_string(uri).map(|p| if p.is_file() { p } else { p.join("Main.bd") })
    });
    let mut compile_plan = None;

    if matches!(kind, GraphKind::ModuleTree | GraphKind::ImportClosure | GraphKind::HostComposition)
        && !is_workspace_manifest_path(&manifest_path)
        && let Some(ctx) = CompilationContext::try_for_analysis_path(&manifest_path, None)
        && let Some(plan) = ctx.compile_plan.clone()
    {
        if entry_path.is_none() {
            entry_path = Some(plan_entry_path(&plan, &plan.source_root));
        }
        effective_roots_from_lockfile_checked(&plan, &plan.project_root.join("Project.lock"))
            .map_err(|error| Error::invalid_params(error.to_string()))?;
        compile_plan = Some(plan);
    }

    let request = GraphFetchRequest {
        kind,
        manifest_path: manifest_path.clone(),
        workspace_manifest,
        compile_plan,
        entry_path,
        entry_source: None,
    };

    let doc = if let Some(db) = db {
        get_graph_document(db, &request).or_else(|_| get_graph_document_simple(&request))
    } else {
        get_graph_document_simple(&request)
    }
    .map_err(|_| missing_args())?;

    Ok(graph_tooling_payload(&doc, kind, project_uri))
}

fn resolve_workspace_manifest(
    manifest_path: &Path,
    kind: GraphKind,
    workspace_uri: Option<&str>,
) -> Result<Option<PathBuf>> {
    if is_workspace_manifest_path(manifest_path) {
        return Ok(Some(manifest_path.to_path_buf()));
    }
    if kind == GraphKind::Workspace {
        return Ok(Some(manifest_path.to_path_buf()));
    }
    workspace_uri.map(super::manifest_path_from_uri).transpose()
}

pub(crate) fn get_project_dependencies(project_uri: &str) -> Result<Value> {
    let manifest_path = super::manifest_path_from_uri(project_uri)?;
    let text = std::fs::read_to_string(&manifest_path).map_err(|_| missing_args())?;
    let manifest = parse_manifest(&text).map_err(|_| missing_args())?;

    let declared: Vec<Value> = manifest
        .dependencies
        .iter()
        .map(|dep| {
            json!({
                "name": dep.name,
                "source": dependency_source_str(dep.source),
                "path": dep.path,
                "url": dep.url,
                "rev": dep.rev,
                "version": dep.version,
                "registry": dep.registry,
            })
        })
        .collect();

    let declared_names: HashSet<&str> = manifest.dependencies.iter().map(|dep| dep.name.as_str()).collect();
    let plan = build_compile_plan(&manifest_path, None).map_err(|error| Error::invalid_params(error.to_string()))?;
    let lock_path = plan.project_root.join("Project.lock");
    let entries = match std::fs::symlink_metadata(&lock_path) {
        Err(source) if source.kind() == ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(Error::invalid_params(format!("failed to read {}: {source}", lock_path.display()))),
        Ok(_) => load_project_lock_dependencies_for_plan(&lock_path, &plan)
            .map_err(|error| Error::invalid_params(error.to_string()))?,
    };
    let lock_entries = entries
        .into_iter()
        .filter_map(|entry| {
            declared_names.contains(entry.name()).then(|| serialize_lock_entry(&entry, &plan)).flatten()
        })
        .collect::<Vec<_>>();
    let locked = lock_entries.iter().map(|(_, value)| value.clone()).collect::<Vec<_>>();

    let locked_names: HashSet<&str> = lock_entries.iter().map(|(name, _)| name.as_str()).collect();
    let mut unresolved: Vec<Value> = declared_names.difference(&locked_names).map(|name| json!(name)).collect();
    unresolved.sort_by(|left, right| left.as_str().unwrap_or_default().cmp(right.as_str().unwrap_or_default()));

    Ok(json!({
        "projectUri": project_uri,
        "declared": declared,
        "locked": locked,
        "unresolved": unresolved,
    }))
}

fn serialize_lock_entry(entry: &ProjectLockDependencyEntry, plan: &CompilePlan) -> Option<(String, Value)> {
    let declared = plan.dependency_projects.iter().find(|dependency| dependency.dependency_name == entry.name());
    let materialized = PortableLockPath::parse(
        "materialized_root",
        entry.materialized_root(),
        PortableLockPathBaseKind::MaterializedRoot,
    )
    .ok()?
    .resolve(&plan.project_root)
    .ok()?;
    let (project, manifest, source_root) = match entry.source() {
        ProjectLockSource::Path | ProjectLockSource::Corelib => declared
            .map(|dependency| {
                (
                    Some(dependency.project_root.display().to_string()),
                    Some(dependency.manifest_path.display().to_string()),
                    Some(dependency.source_root.display().to_string()),
                )
            })
            .unwrap_or((None, None, None)),
        ProjectLockSource::Registry => (None, None, None),
    };
    Some((
        entry.name().to_string(),
        json!({
            "name": entry.name(),
            "manifest": manifest,
            "project": project,
            "sourceRoot": source_root,
            "materializedRoot": materialized.display().to_string(),
            "resolvedVersion": entry.resolved_version(),
            "registry": entry.registry(),
        }),
    ))
}

fn dependency_source_str(source: DependencySource) -> &'static str {
    match source {
        DependencySource::Path => "path",
        DependencySource::Git => "git",
        DependencySource::Registry => "registry",
    }
}

pub(crate) fn graph_kind_from_args(arguments: Option<&[Value]>) -> GraphKind {
    let Some(obj) = arguments.and_then(|args| args.first()).and_then(Value::as_object) else {
        return GraphKind::ProjectDeps;
    };
    obj.get("kind").and_then(Value::as_str).and_then(GraphKind::parse).unwrap_or(GraphKind::ProjectDeps)
}

pub(crate) fn optional_uri_arg(arguments: Option<&[Value]>, key: &str) -> Option<String> {
    arguments
        .and_then(|args| args.first())
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}
