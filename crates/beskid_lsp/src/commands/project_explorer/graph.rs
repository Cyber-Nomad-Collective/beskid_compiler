//! Project graph and dependency execute-command payloads.

use beskid_analysis::projects::{
    load_project_lock_dependencies, parse_manifest, DependencySource, ProjectLockDependencyEntry,
};
use beskid_graph::{graph_tooling_payload, GraphKind};
use beskid_queries::{get_graph_document, get_graph_document_simple, GraphFetchRequest};
use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::Result;

use crate::protocol::execute_args::missing_args;

pub(crate) fn get_graph(
    project_uri: &str,
    kind: GraphKind,
    entry_uri: Option<&str>,
    workspace_uri: Option<&str>,
    db: Option<&mut beskid_queries::BeskidDatabase>,
) -> Result<Value> {
    let manifest_path = super::manifest_path_from_uri(project_uri)?;
    let request = GraphFetchRequest {
        kind,
        manifest_path: manifest_path.clone(),
        workspace_manifest: workspace_uri.map(super::manifest_path_from_uri).transpose()?,
        compile_plan: None,
        entry_path: entry_uri.and_then(|uri| {
            crate::workspace_scan::path_from_uri_string(uri).map(|p| {
                if p.is_file() {
                    p
                } else {
                    p.join("Main.bd")
                }
            })
        }),
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

    let project_root = manifest_path.parent().ok_or_else(missing_args)?;
    let locked = load_project_lock_dependencies(project_root)
        .unwrap_or_default()
        .iter()
        .map(serialize_lock_entry)
        .collect::<Vec<_>>();

    let unresolved: Vec<Value> = Vec::new();

    Ok(json!({
        "projectUri": project_uri,
        "declared": declared,
        "locked": locked,
        "unresolved": unresolved,
    }))
}

fn serialize_lock_entry(entry: &ProjectLockDependencyEntry) -> Value {
    json!({
        "name": entry.name(),
        "manifest": entry.manifest(),
        "project": entry.project(),
        "sourceRoot": entry.source_root(),
        "materializedRoot": entry.materialized_root(),
        "resolvedVersion": entry.resolved_version(),
        "registry": entry.registry(),
    })
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
    obj.get("kind")
        .and_then(Value::as_str)
        .and_then(GraphKind::parse)
        .unwrap_or(GraphKind::ProjectDeps)
}

pub(crate) fn optional_uri_arg(arguments: Option<&[Value]>, key: &str) -> Option<String> {
    arguments
        .and_then(|args| args.first())
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}
