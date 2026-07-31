//! Salsa-backed Mermaid graph queries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use beskid_analysis::projects::graph::build_project_graph_with_options;
use beskid_analysis::projects::model::AssemblyOptions;
use beskid_analysis::projects::{
    CompilePlan, ProjectGraphBuildOptions, parse_workspace_manifest, project_manifest_for_member_dir,
};
use beskid_analysis::services::{parse_program_with_source_name, resolve_program_composition};
use beskid_graph::{
    GraphDocument, GraphKind, from_composition, from_import_closure, from_module_graph, from_project_graph,
    from_workspace,
};
use thiserror::Error;

use crate::db::{BeskidDatabase, Db};
use crate::graph::program_assembly;
use crate::inputs::ProjectSession;
use crate::modhost::ManifestGenerationId;
use crate::stats::{record_query_hit, record_query_miss};

#[derive(Debug, Error)]
pub enum GraphQueryError {
    #[error("failed to build project graph: {0}")]
    Project(#[from] beskid_analysis::projects::ProjectError),
    #[error("failed to assemble program: {0}")]
    Assembly(#[from] beskid_analysis::projects::AssemblyError),
    #[error("failed to render graph: {0}")]
    Render(#[from] beskid_graph::GraphError),
    #[error("failed to parse program: {0}")]
    Parse(String),
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone)]
pub struct GraphFetchRequest {
    pub kind: GraphKind,
    pub manifest_path: PathBuf,
    pub workspace_manifest: Option<PathBuf>,
    pub compile_plan: Option<CompilePlan>,
    pub entry_path: Option<PathBuf>,
    pub entry_source: Option<String>,
}

/// Fetch a graph document; uses Salsa when a compile plan is available.
pub fn get_graph_document(
    db: &mut BeskidDatabase,
    request: &GraphFetchRequest,
) -> Result<GraphDocument, GraphQueryError> {
    match request.kind {
        GraphKind::ProjectDeps => {
            let manifest_gen = ensure_manifest_generation(db, &request.manifest_path);
            let session = minimal_session(db, &request.manifest_path);
            Ok((*graph_mermaid_project_deps(db, session, manifest_gen, request.manifest_path.display().to_string()))
                .clone())
        }
        GraphKind::Workspace => {
            let workspace = request.workspace_manifest.as_ref().ok_or_else(|| {
                GraphQueryError::Message("workspace manifest required for workspace graph".to_owned())
            })?;
            let session = minimal_session(db, &request.manifest_path);
            let manifest_gen = ensure_manifest_generation(db, workspace);
            Ok((*graph_mermaid_workspace(db, session, manifest_gen, workspace.display().to_string())).clone())
        }
        GraphKind::ModuleTree | GraphKind::ImportClosure | GraphKind::HostComposition => {
            build_assembly_graph(db, request)
        }
    }
}

/// Fetch without requiring `BeskidDatabase` (project/workspace only).
pub fn get_graph_document_simple(request: &GraphFetchRequest) -> Result<GraphDocument, GraphQueryError> {
    match request.kind {
        GraphKind::ProjectDeps => {
            let graph = build_project_graph_with_options(&request.manifest_path, ProjectGraphBuildOptions::default())?;
            from_project_graph(&graph).map_err(GraphQueryError::from)
        }
        GraphKind::Workspace => workspace_graph(request),
        _ => Err(GraphQueryError::Message("database session required for this graph kind".to_owned())),
    }
}

fn build_assembly_graph(
    db: &mut BeskidDatabase,
    request: &GraphFetchRequest,
) -> Result<GraphDocument, GraphQueryError> {
    let plan =
        request.compile_plan.as_ref().ok_or_else(|| GraphQueryError::Message("compile plan required".to_owned()))?;
    let entry =
        request.entry_path.as_ref().ok_or_else(|| GraphQueryError::Message("entry path required".to_owned()))?;

    match request.kind {
        GraphKind::ModuleTree => {
            let assembly =
                program_assembly(db, plan, None, entry, request.entry_source.as_deref(), &AssemblyOptions::default())?;
            from_module_graph(assembly.module_index.module_graph()).map_err(GraphQueryError::from)
        }
        GraphKind::ImportClosure => {
            let session = db.ensure_project_session(plan, entry, manifest_digest(&request.manifest_path));
            let assembly =
                program_assembly(db, plan, None, entry, request.entry_source.as_deref(), &AssemblyOptions::default())?;
            let grammar = db.grammar_revision();
            let units = assembly
                .units
                .iter()
                .map(|unit| {
                    let imports = crate::unit::unit_imports(db, session, grammar, unit.path.clone());
                    (unit.path.clone(), imports)
                })
                .collect::<Vec<_>>();
            from_import_closure(&units).map_err(GraphQueryError::from)
        }
        GraphKind::HostComposition => {
            let source = request
                .entry_source
                .clone()
                .or_else(|| std::fs::read_to_string(entry).ok())
                .ok_or_else(|| GraphQueryError::Message("entry source required for host graph".to_owned()))?;
            let program = parse_program_with_source_name(&entry.display().to_string(), &source)
                .map_err(|e| GraphQueryError::Parse(e.to_string()))?;
            let composition = resolve_program_composition(&program, Some(plan));
            from_composition(&composition.snapshot, &composition.snapshot.registrations, &composition.dependency_edges)
                .map_err(GraphQueryError::from)
        }
        _ => unreachable!(),
    }
}

fn workspace_graph(request: &GraphFetchRequest) -> Result<GraphDocument, GraphQueryError> {
    let workspace = request
        .workspace_manifest
        .as_ref()
        .ok_or_else(|| GraphQueryError::Message("workspace manifest required".to_owned()))?;
    let text = std::fs::read_to_string(workspace).map_err(|e| GraphQueryError::Message(e.to_string()))?;
    let manifest = parse_workspace_manifest(&text).map_err(|e| GraphQueryError::Message(e.to_string()))?;
    let workspace_dir =
        workspace.parent().ok_or_else(|| GraphQueryError::Message("workspace dir missing".to_owned()))?;
    let mut members = Vec::new();
    for member in &manifest.members {
        let member_dir = workspace_dir.join(&member.path);
        let Ok(member_manifest) = project_manifest_for_member_dir(&member_dir) else {
            continue;
        };
        let graph = build_project_graph_with_options(&member_manifest, ProjectGraphBuildOptions::default())?;
        members.push((member.name.clone(), graph));
    }
    from_workspace(&manifest.workspace.name, &members).map_err(GraphQueryError::from)
}

#[salsa::tracked]
pub fn graph_fingerprint_project_deps(
    db: &dyn Db,
    _session: ProjectSession,
    manifest_gen: ManifestGenerationId,
    manifest_path: String,
) -> String {
    let _ = (db, manifest_gen);
    record_query_miss();
    format!("project:{manifest_path}")
}

#[salsa::tracked]
pub fn graph_mermaid_project_deps(
    db: &dyn Db,
    session: ProjectSession,
    manifest_gen: ManifestGenerationId,
    manifest_path: String,
) -> Arc<GraphDocument> {
    let _ = graph_fingerprint_project_deps(db, session, manifest_gen, manifest_path.clone());
    record_query_hit();
    let graph = match build_project_graph_with_options(Path::new(&manifest_path), ProjectGraphBuildOptions::default()) {
        Ok(g) => g,
        Err(e) => {
            return Arc::new(GraphDocument::empty(GraphKind::ProjectDeps, &e.to_string()));
        }
    };
    Arc::new(from_project_graph(&graph).expect("mermaid"))
}

#[salsa::tracked]
pub fn graph_mermaid_workspace(
    db: &dyn Db,
    session: ProjectSession,
    manifest_gen: ManifestGenerationId,
    workspace_manifest: String,
) -> Arc<GraphDocument> {
    let _ = (db, session, manifest_gen);
    record_query_miss();
    let path = Path::new(&workspace_manifest);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return Arc::new(GraphDocument::empty(GraphKind::Workspace, &e.to_string())),
    };
    let manifest = match parse_workspace_manifest(&text) {
        Ok(m) => m,
        Err(e) => return Arc::new(GraphDocument::empty(GraphKind::Workspace, &e.to_string())),
    };
    let Some(workspace_dir) = path.parent() else {
        return Arc::new(GraphDocument::empty(GraphKind::Workspace, "missing parent directory"));
    };
    let mut members = Vec::new();
    for member in &manifest.members {
        let member_dir = workspace_dir.join(&member.path);
        let Ok(member_manifest) = project_manifest_for_member_dir(&member_dir) else {
            continue;
        };
        let graph = match build_project_graph_with_options(&member_manifest, ProjectGraphBuildOptions::default()) {
            Ok(g) => g,
            Err(_) => continue,
        };
        members.push((member.name.clone(), graph));
    }
    Arc::new(from_workspace(&manifest.workspace.name, &members).expect("workspace mermaid"))
}

pub fn ensure_manifest_generation(db: &mut BeskidDatabase, manifest_path: &Path) -> ManifestGenerationId {
    ManifestGenerationId::new(db, manifest_digest(manifest_path))
}

pub fn manifest_digest(manifest_path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    manifest_path.hash(&mut hasher);
    if let Ok(bytes) = std::fs::read(manifest_path) {
        bytes.hash(&mut hasher);
    }
    if let Some(parent) = manifest_path.parent() {
        let lock = parent.join("Project.lock");
        if let Ok(bytes) = std::fs::read(lock) {
            bytes.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

fn minimal_session(db: &mut BeskidDatabase, manifest_path: &Path) -> ProjectSession {
    let project_root = manifest_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    db.ensure_project_session(
        &CompilePlan {
            project_name: String::new(),
            project_root: project_root.clone(),
            manifest_path: manifest_path.to_path_buf(),
            source_root: project_root.join("src"),
            target: beskid_analysis::projects::Target {
                name: "graph".to_owned(),
                kind: beskid_analysis::projects::TargetKind::App,
                entry: Some("Main.bd".to_owned()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        },
        manifest_path,
        manifest_digest(manifest_path),
    )
}
