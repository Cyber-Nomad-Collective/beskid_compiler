use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::Uri;

use beskid_analysis::services::{
    PrepareOptions, ResolvedInput, SessionFingerprint, build_document_analysis_from_resolution,
    build_document_analysis_with_context, parse_program_with_source_name, resolve_input,
    resolved_input_from_plan,
};
use beskid_queries::{
    AstNodeKey, SyntaxGenerationId, build_typed_program, bump_file_revision,
    bump_typed_prepare_revision, fingerprint_key, node_span, resolved_item, resolved_local,
    typed_entry_state_with_db,
};

use crate::manifest_uri::is_manifest_uri;
use crate::session::db_access::with_compilation_db_mut_state;
use crate::session::diagnostics_bridge::analyze_document_for_state;
use crate::session::project_context::cached_compilation_context;
use crate::session::startup::wait_for_initial_scan;
use crate::session::store::{Document, State, SyntaxDefinition, SyntaxHover, SyntaxSymbol};
use crate::workspace_scan::uri_to_path;

/// Document analysis snapshot shape; bump when snapshot fields change.
pub const ANALYSIS_CACHE_VERSION: u32 = 5;

/// Debounce window for typed executable prepare (coalesced with diagnostic publish).
const TYPED_PREPARE_DEBOUNCE_MS: u64 = 120;

fn salsa_revision(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn entry_key_for_resolved(resolved: &ResolvedInput) -> Option<String> {
    let plan = resolved.compile_plan.as_ref()?;
    Some(fingerprint_key(&SessionFingerprint::for_entry(
        plan,
        &resolved.source_path,
    )))
}

fn module_paths_from_resolution(resolution: &beskid_analysis::resolve::Resolution) -> HashSet<String> {
    resolution
        .module_graph
        .modules()
        .iter()
        .filter_map(|module| {
            if module.path.is_empty() {
                None
            } else {
                Some(module.path.join("::"))
            }
        })
        .collect()
}

fn lockfile_digest_for_plan(plan: &beskid_analysis::projects::CompilePlan) -> String {
    let mut hasher = DefaultHasher::new();
    plan.project_root.hash(&mut hasher);
    plan.target.entry.hash(&mut hasher);
    plan.target.name.hash(&mut hasher);
    if let Ok(bytes) = std::fs::read(plan.project_root.join("Project.lock")) {
        bytes.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn syntax_facts_for_entry(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
    entry_state: &beskid_queries::TypedEntryState,
) -> (Vec<SyntaxDefinition>, Vec<SyntaxHover>, Vec<SyntaxSymbol>) {
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let Some(front_end) = entry_state.typed.as_ref() else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let project = db.ensure_project_session(
        plan,
        &resolved.source_path,
        lockfile_digest_for_plan(plan),
    );
    let assembly = Arc::new(beskid_analysis::projects::SyntaxProgramAssembly::from(
        &front_end.assembly,
    ));
    let generation = SyntaxGenerationId(entry_state.generation);
    let Ok(typed) = build_typed_program(db, project, generation, assembly) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let unit = typed.entry;
    let Some(entry) = typed.assembly.units.get(typed.assembly.entry_index) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let index = beskid_analysis::syntax_query::SyntaxIndex::from_program(&entry.program, generation);

    let mut definitions = Vec::new();
    let mut hovers = Vec::new();
    for metadata in index.metadata() {
        let reference = AstNodeKey {
            unit,
            generation,
            node: metadata.id,
        };
        let local = resolved_local(db, reference).ok().flatten();
        let declaration = local
            .map(|resolved| resolved.declaration)
            .or_else(|| {
                resolved_item(db, reference)
                    .ok()
                    .flatten()
                    .map(|resolved| resolved.declaration)
            });
        let Some(declaration) = declaration else {
            continue;
        };
        let Some(reference_span) = node_span(db, reference).ok().flatten() else {
            continue;
        };
        let Some(declaration_span) = node_span(db, declaration).ok().flatten() else {
            continue;
        };
        let declaration_path = declaration.unit.path(db).clone();
        definitions.push(SyntaxDefinition {
            reference_start: reference_span.start,
            reference_end: reference_span.end,
            declaration_path: declaration_path.clone(),
            declaration_start: declaration_span.start,
            declaration_end: declaration_span.end,
        });
        let Some(target_unit) = typed
            .assembly
            .units
            .iter()
            .find(|candidate| candidate.path == declaration_path)
        else {
            continue;
        };
        let target_index = beskid_analysis::syntax_query::SyntaxIndex::from_program(
            &target_unit.program,
            generation,
        );
        let (location_start, location_end, name, kind) = target_index
            .node_at(&target_unit.program, declaration.node)
            .and_then(|node| {
                node.of::<beskid_analysis::syntax::FunctionDefinition>()
                    .map(|function| {
                        (
                            function.name.span.start,
                            function.name.span.end,
                            function.name.node.name.clone(),
                            "function",
                        )
                    })
            })
            .unwrap_or_else(|| {
                let name = entry
                    .source
                    .get(reference_span.start..reference_span.end)
                    .unwrap_or_default()
                    .to_string();
                (
                    declaration_span.start,
                    declaration_span.end,
                    name,
                    "local",
                )
            });
        if !name.is_empty() {
            hovers.push(SyntaxHover {
                reference_start: reference_span.start,
                reference_end: reference_span.end,
                markdown: format!("**{kind}** `{name}`"),
                location_path: declaration_path,
                location_start,
                location_end,
            });
        }
    }
    definitions.sort_by_key(|definition| {
        (definition.reference_start, definition.reference_end, definition.declaration_path.clone())
    });
    definitions.dedup();
    hovers.sort_by_key(|hover| (hover.reference_start, hover.reference_end, hover.location_path.clone()));
    hovers.dedup();
    (definitions, hovers, syntax_symbols_for_program(&entry.program))
}

fn syntax_symbols_for_program(program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>) -> Vec<SyntaxSymbol> {
    use beskid_analysis::services::AnalysisSymbolKind as Kind;
    use beskid_analysis::syntax::Node;
    program.node.items.iter().filter_map(|item| match &item.node {
        Node::Function(definition) => Some((definition.node.name.node.name.clone(), Kind::Function, definition.node.name.span)),
        Node::Method(definition) => Some((definition.node.name.node.name.clone(), Kind::Method, definition.node.name.span)),
        Node::TestDefinition(definition) => Some((definition.node.name.node.name.clone(), Kind::Test, definition.node.name.span)),
        Node::TypeDefinition(definition) => Some((definition.node.name.node.name.clone(), Kind::Type, definition.node.name.span)),
        Node::EnumDefinition(definition) => Some((definition.node.name.node.name.clone(), Kind::Enum, definition.node.name.span)),
        Node::ContractDefinition(definition) => Some((definition.node.name.node.name.clone(), Kind::Contract, definition.node.name.span)),
        Node::InlineModule(definition) => Some((definition.node.name.node.name.clone(), Kind::Module, definition.node.name.span)),
        Node::ModuleDeclaration(definition) => definition.node.path.node.segments.last().map(|segment| (segment.node.name.node.name.clone(), Kind::Module, segment.span)),
        Node::UseDeclaration(definition) => definition.node.alias.as_ref().map(|alias| (alias.node.name.clone(), Kind::Use, alias.span)).or_else(|| definition.node.path.node.segments.last().map(|segment| (segment.node.name.node.name.clone(), Kind::Use, segment.span))),
        _ => None,
    }.map(|(name, kind, span)| SyntaxSymbol { name, kind, start: span.start, end: span.end })).collect()
}

fn bump_entry_file_revision(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
) {
    if let Some(entry_key) = entry_key_for_resolved(resolved) {
        bump_file_revision(db, &entry_key);
    }
}

fn bump_entry_typed_prepare_revision(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
) {
    if let Some(entry_key) = entry_key_for_resolved(resolved) {
        bump_typed_prepare_revision(db, &entry_key);
    }
}

async fn resolved_input_for_path(
    state: &RwLock<State>,
    path: &Path,
    text: &str,
) -> Option<(ResolvedInput, beskid_analysis::CompilationContext)> {
    let session = cached_compilation_context(state, path).await?;
    session.compile_plan.as_ref()?;
    let mut resolved = resolve_input(
        Some(&path.to_path_buf()),
        None,
        None,
        None,
        false,
        false,
    )
    .ok()
    .or_else(|| {
        let plan = session.compile_plan.clone()?;
        Some(resolved_input_from_plan(
            path.to_path_buf(),
            text.to_string(),
            plan,
            None,
            None,
        ))
    })?;
    resolved.source = text.to_string();
    Some((resolved, session))
}

async fn build_document_analysis(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
) -> Option<beskid_analysis::services::DocumentAnalysisSnapshot> {
    wait_for_initial_scan(state).await;

    if is_manifest_uri(uri) {
        return None;
    }

    let path = uri_to_path(uri)?;
    let program =
        parse_program_with_source_name(&uri.to_string(), text).ok()?;

    let (resolved, session) = match resolved_input_for_path(state, &path, text).await {
        Some(pair) => pair,
        None => {
            return Some(build_document_analysis_with_context(
                &program,
                uri.to_string(),
                text,
                &path,
                None,
                None,
            ));
        }
    };

    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = session.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path.clone(), text.to_string());

        let options = PrepareOptions::default();
        if let Ok(entry_state) = typed_entry_state_with_db(db, &resolved, &options, None) {
            let resolution = (*entry_state.resolution.0).clone();
            let module_paths = module_paths_from_resolution(&resolution);
            return Some(build_document_analysis_from_resolution(
                &program,
                uri.to_string(),
                text,
                &path,
                Some(resolution),
                module_paths,
                session.compile_plan.as_ref(),
                None,
            ));
        }

        Some(build_document_analysis_with_context(
            &program,
            uri.to_string(),
            text,
            &path,
            Some(&session),
            None,
        ))
    })
    .await
}

async fn build_syntax_facts(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
) -> (Vec<SyntaxDefinition>, Vec<SyntaxHover>, Vec<SyntaxSymbol>) {
    wait_for_initial_scan(state).await;
    if is_manifest_uri(uri) {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let Some(path) = uri_to_path(uri) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = session.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path, text.to_string());
        let options = PrepareOptions::default();
        let Ok(entry_state) = typed_entry_state_with_db(db, &resolved, &options, None) else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        syntax_facts_for_entry(db, &resolved, &entry_state)
    })
    .await
}

/// Build a [`Document`] for `uri`, attaching a fresh analysis snapshot when possible.
pub async fn build_document(
    state: &RwLock<State>,
    uri: &Uri,
    version: i32,
    text: String,
) -> Document {
    let analysis = build_document_analysis(state, uri, &text).await;
    let (syntax_definitions, syntax_hovers, syntax_symbols) = build_syntax_facts(state, uri, &text).await;
    Document {
        version,
        text,
        analysis_cache_version: ANALYSIS_CACHE_VERSION,
        analysis,
        syntax_definitions,
        syntax_hovers,
        syntax_symbols,
    }
}

/// Store a disk-backed snapshot when the URI is not already an open buffer.
pub async fn set_disk_snapshot(state: &RwLock<State>, uri: Uri, doc: Document) {
    let mut write_state = state.write().await;
    if write_state.docs.contains_key(&uri) {
        return;
    }
    write_state.workspace_index.insert(uri, doc);
}

async fn touch_entry_file_revision_for_uri(state: &RwLock<State>, uri: &Uri, text: &str) {
    wait_for_initial_scan(state).await;

    let Some(path) = uri_to_path(uri) else {
        return;
    };
    let Some((resolved, _)) = resolved_input_for_path(state, &path, text).await else {
        return;
    };
    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = resolved.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path, text.to_string());
        bump_entry_file_revision(db, &resolved);
    })
    .await;
}

async fn apply_typed_prepare_rebuild(state: &RwLock<State>, uri: &Uri) {
    wait_for_initial_scan(state).await;

    let text = {
        let read = state.read().await;
        read.docs
            .get(uri)
            .map(|doc| doc.text.clone())
            .or_else(|| read.workspace_index.get(uri).map(|doc| doc.text.clone()))
    };
    let Some(text) = text else {
        return;
    };

    let Some(path) = uri_to_path(uri) else {
        return;
    };
    let Some((resolved, _)) = resolved_input_for_path(state, &path, &text).await else {
        return;
    };

    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = resolved.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        bump_entry_typed_prepare_revision(db, &resolved);
    })
    .await;

    let analysis = build_document_analysis(state, uri, &text).await;
    let (syntax_definitions, syntax_hovers, syntax_symbols) = build_syntax_facts(state, uri, &text).await;
    let mut write = state.write().await;
    if let Some(doc) = write.docs.get_mut(uri)
        && doc.text == text
    {
        doc.analysis = analysis;
        doc.syntax_definitions = syntax_definitions;
        doc.syntax_hovers = syntax_hovers;
        doc.syntax_symbols = syntax_symbols;
        doc.analysis_cache_version = ANALYSIS_CACHE_VERSION;
    } else if let Some(doc) = write.workspace_index.get_mut(uri)
        && doc.text == text
    {
        doc.analysis = analysis;
        doc.syntax_definitions = syntax_definitions;
        doc.syntax_hovers = syntax_hovers;
        doc.syntax_symbols = syntax_symbols;
        doc.analysis_cache_version = ANALYSIS_CACHE_VERSION;
    }
}

/// Schedule debounced typed executable prepare after buffer edits (120ms coalescing).
pub async fn schedule_typed_prepare_rebuild(state: Arc<RwLock<State>>, uri: Uri) {
    let rev = {
        let mut write = state.write().await;
        let next = write
            .typed_prepare_schedule_revision
            .get(&uri)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        write.typed_prepare_schedule_revision.insert(uri.clone(), next);
        next
    };

    let state_for_task = state.clone();
    let uri_for_task = uri.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(TYPED_PREPARE_DEBOUNCE_MS)).await;
        let should_run = {
            let read = state_for_task.read().await;
            read.typed_prepare_schedule_revision
                .get(&uri_for_task)
                .copied()
                == Some(rev)
        };
        if should_run {
            apply_typed_prepare_rebuild(&state_for_task, &uri_for_task).await;
        }
    });
}

/// Upsert an open document, respecting monotonic versions and Salsa revision fast paths.
pub async fn set_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) {
    let revision = salsa_revision(&text);
    let mut write_state = state.write().await;
    write_state.workspace_index.remove(&uri);

    if let Some(existing) = write_state.docs.get_mut(&uri) {
        if version < existing.version {
            return;
        }

        if existing.analysis_cache_version == ANALYSIS_CACHE_VERSION
            && salsa_revision(&existing.text) == revision
        {
            existing.version = version;
            existing.text = text;
            return;
        }
    }

    drop(write_state);
    touch_entry_file_revision_for_uri(state, &uri, &text).await;
    let analysis = build_document_analysis(state, &uri, &text).await;
    let (syntax_definitions, syntax_hovers, syntax_symbols) = build_syntax_facts(state, &uri, &text).await;

    let mut write_state = state.write().await;
    write_state.docs.insert(
        uri,
        Document {
            version,
            text,
            analysis_cache_version: ANALYSIS_CACHE_VERSION,
            analysis,
            syntax_definitions,
            syntax_hovers,
            syntax_symbols,
        },
    );
}

/// Drop an open buffer after `didClose` (disk hydration may repopulate the workspace index).
pub async fn remove_document(state: &RwLock<State>, uri: &Uri) {
    let mut write = state.write().await;
    write.docs.remove(uri);
    write.typed_prepare_schedule_revision.remove(uri);
}

/// Rebuild analysis snapshots for open `.bd` buffers after compilation context / assembly invalidation.
pub async fn rebuild_open_document_analysis(state: &RwLock<State>) {
    let entries: Vec<(Uri, String)> = {
        let read = state.read().await;
        read.docs
            .iter()
            .filter(|(uri, _)| !is_manifest_uri(uri))
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect()
    };

    for (uri, text) in entries {
        let analysis = build_document_analysis(state, &uri, &text).await;
        let (syntax_definitions, syntax_hovers, syntax_symbols) = build_syntax_facts(state, &uri, &text).await;
        let mut write = state.write().await;
        if let Some(doc) = write.docs.get_mut(&uri)
            && doc.text == text
        {
            doc.analysis = analysis;
            doc.syntax_definitions = syntax_definitions;
            doc.syntax_hovers = syntax_hovers;
            doc.syntax_symbols = syntax_symbols;
            doc.analysis_cache_version = ANALYSIS_CACHE_VERSION;
        }
    }
}

/// Recompute diagnostics for the union of open buffer or workspace snapshot and push to the client.
pub async fn publish_diagnostics_for_uri(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let snapshot = {
        let state = state.read().await;
        state.document_union(uri)
    };

    let Some(doc) = snapshot else {
        return;
    };

    let compilation_context = if let Some(path) = uri_to_path(uri) {
        cached_compilation_context(state, &path).await
    } else {
        None
    };
    let diagnostics = analyze_document_for_state(
        state,
        uri,
        &doc.text,
        doc.analysis.as_ref(),
        compilation_context.as_ref(),
    )
    .await;
    client
        .publish_diagnostics(uri.clone(), diagnostics, Some(doc.version))
        .await;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::Uri;

    use super::{ANALYSIS_CACHE_VERSION, set_document};
    use crate::session::store::{Document, State};

    fn source() -> String {
        "i32 Main() { return 0; }".to_string()
    }

    fn uri() -> Uri {
        Uri::from_str("file:///cache_test.bd").expect("valid uri")
    }

    #[tokio::test]
    async fn set_document_ignores_stale_versions() {
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        let file_uri = uri();
        set_document(&state, file_uri.clone(), 2, source()).await;
        set_document(
            &state,
            file_uri.clone(),
            1,
            "i32 Main() { return 1; }".to_string(),
        )
        .await;

        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert_eq!(doc.version, 2);
        assert_eq!(doc.text, source());
    }

    #[tokio::test]
    async fn set_document_rebuilds_when_cache_version_changes() {
        let file_uri = uri();
        let text = source();
        let mut state = State::default();
        state.docs.insert(
            file_uri.clone(),
            Document {
                version: 1,
                text: text.clone(),
                analysis_cache_version: ANALYSIS_CACHE_VERSION.saturating_sub(1),
                analysis: None,
                syntax_definitions: Vec::new(),
                syntax_hovers: Vec::new(),
                syntax_symbols: Vec::new(),
            },
        );

        state.mark_initial_scan_complete();
        let state = tokio::sync::RwLock::new(state);
        set_document(&state, file_uri.clone(), 2, text).await;

        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert_eq!(doc.version, 2);
        assert_eq!(doc.analysis_cache_version, ANALYSIS_CACHE_VERSION);
        assert!(doc.analysis.is_some());
    }
}
