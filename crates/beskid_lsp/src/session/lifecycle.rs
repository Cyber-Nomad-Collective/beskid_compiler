use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::Uri;

use beskid_analysis::services::{
    PrepareOptions, ResolvedInput, SessionFingerprint, resolve_input, resolved_input_from_plan,
};
use beskid_queries::{
    AstNodeKey, SemanticTypeId, SyntaxGenerationId, build_typed_program, bump_file_revision,
    bump_typed_prepare_revision, fingerprint_key, node_span, node_type, resolved_item,
    resolved_local, typed_entry_state_with_db,
};

use crate::diagnostics::lsp_diagnostics_from_syntax;
use crate::manifest_uri::is_manifest_uri;
use crate::session::diagnostics_bridge::collect_syntax_diagnostics_for_state;
use crate::session::documentation_facts::{
    SyntaxDocumentationFact, syntax_documentation_facts_for_source,
};
use crate::session::db_access::with_compilation_db_mut_state;
use crate::session::project_context::cached_compilation_context;
use crate::session::startup::wait_for_initial_scan;
use crate::session::store::{
    Document, State, SyntaxCompletion, SyntaxDefinition, SyntaxDiagnostic, SyntaxHover,
    SyntaxInlayHint, SyntaxSymbol,
};
use crate::workspace_scan::uri_to_path;

/// Debounce window for typed executable prepare (coalesced with diagnostic publish).
const TYPED_PREPARE_DEBOUNCE_MS: u64 = 120;

/// Syntax-only LSP facts for one prepared entry revision.
///
/// Keeping the facts named prevents lifecycle refresh paths from silently
/// reordering independent syntax-derived capabilities.
#[derive(Default)]
struct SyntaxFacts {
    definitions: Vec<SyntaxDefinition>,
    hovers: Vec<SyntaxHover>,
    symbols: Vec<SyntaxSymbol>,
    completion: Option<SyntaxCompletion>,
    inlay_hints: Vec<SyntaxInlayHint>,
    documentation: Vec<SyntaxDocumentationFact>,
    diagnostics: Vec<SyntaxDiagnostic>,
}

fn entry_key_for_resolved(resolved: &ResolvedInput) -> Option<String> {
    let plan = resolved.compile_plan.as_ref()?;
    Some(fingerprint_key(&SessionFingerprint::for_entry(
        plan,
        &resolved.source_path,
    )))
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
) -> SyntaxFacts {
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return SyntaxFacts::default();
    };
    let Some(front_end) = entry_state.typed.as_ref() else {
        return SyntaxFacts::default();
    };
    let project =
        db.ensure_project_session(plan, &resolved.source_path, lockfile_digest_for_plan(plan));
    // Fail closed to prepare-spine syntax authority: post-mod-rewrite entry program, never the
    // pre-rewrite ProgramAssembly units that still carry HIR compatibility state.
    let assembly = Arc::new(front_end.syntax_assembly());
    let generation = SyntaxGenerationId(entry_state.generation);
    let Ok(typed) = build_typed_program(db, project, generation, assembly) else {
        return SyntaxFacts::default();
    };
    let unit = typed.entry;
    let Some(entry) = typed.assembly.units().get(typed.assembly.entry_index()) else {
        return SyntaxFacts::default();
    };
    let index =
        beskid_analysis::syntax_query::SyntaxIndex::from_program(&entry.program, generation);

    let mut definitions = Vec::new();
    let mut hovers = Vec::new();
    let mut inlay_hints = Vec::new();
    for metadata in index.metadata() {
        let reference = AstNodeKey {
            unit,
            generation,
            node: metadata.id,
        };
        if matches!(
            metadata.kind,
            beskid_analysis::syntax_query::NodeKind::LiteralExpression
                | beskid_analysis::syntax_query::NodeKind::PathExpression
        ) && let Some(type_label) = node_type(db, reference)
            .ok()
            .flatten()
            .and_then(syntax_type_label)
            && let Some(span) = node_span(db, reference).ok().flatten()
        {
            inlay_hints.push(SyntaxInlayHint {
                start: span.start,
                end: span.end,
                type_label: type_label.to_string(),
            });
        }
        let local = resolved_local(db, reference).ok().flatten();
        let declaration = local.map(|resolved| resolved.declaration).or_else(|| {
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
            .units()
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
                (declaration_span.start, declaration_span.end, name, "local")
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
        (
            definition.reference_start,
            definition.reference_end,
            definition.declaration_path.clone(),
        )
    });
    definitions.dedup();
    hovers.sort_by_key(|hover| {
        (
            hover.reference_start,
            hover.reference_end,
            hover.location_path.clone(),
        )
    });
    hovers.dedup();
    inlay_hints.sort_by_key(|hint| (hint.start, hint.end, hint.type_label.clone()));
    inlay_hints.dedup();
    let completion = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::Program)
        .next()
        .map(|node| SyntaxCompletion {
            anchor: AstNodeKey {
                unit,
                generation,
                node,
            },
        });
    SyntaxFacts {
        definitions,
        hovers,
        symbols: syntax_symbols_for_program(&entry.program),
        completion,
        inlay_hints,
        documentation: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn syntax_type_label(ty: SemanticTypeId) -> Option<&'static str> {
    match ty {
        SemanticTypeId::UNIT => Some("unit"),
        SemanticTypeId::BOOL => Some("bool"),
        SemanticTypeId::I32 => Some("i32"),
        SemanticTypeId::I64 => Some("i64"),
        SemanticTypeId::U8 => Some("u8"),
        SemanticTypeId::F64 => Some("f64"),
        SemanticTypeId::CHAR => Some("char"),
        SemanticTypeId::STRING => Some("string"),
        SemanticTypeId::WORD => Some("word"),
        SemanticTypeId::POINTER => Some("pointer"),
        SemanticTypeId::NEVER => Some("never"),
        _ => None,
    }
}

fn syntax_symbols_for_program(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Vec<SyntaxSymbol> {
    use beskid_analysis::services::AnalysisSymbolKind as Kind;
    use beskid_analysis::syntax::Node;
    program
        .node
        .items
        .iter()
        .filter_map(|item| {
            match &item.node {
                Node::Function(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Function,
                    definition.node.name.span,
                )),
                Node::Method(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Method,
                    definition.node.name.span,
                )),
                Node::TestDefinition(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Test,
                    definition.node.name.span,
                )),
                Node::TypeDefinition(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Type,
                    definition.node.name.span,
                )),
                Node::EnumDefinition(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Enum,
                    definition.node.name.span,
                )),
                Node::ContractDefinition(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Contract,
                    definition.node.name.span,
                )),
                Node::InlineModule(definition) => Some((
                    definition.node.name.node.name.clone(),
                    Kind::Module,
                    definition.node.name.span,
                )),
                Node::ModuleDeclaration(definition) => {
                    definition.node.path.node.segments.last().map(|segment| {
                        (
                            segment.node.name.node.name.clone(),
                            Kind::Module,
                            segment.span,
                        )
                    })
                }
                Node::UseDeclaration(definition) => definition
                    .node
                    .alias
                    .as_ref()
                    .map(|alias| (alias.node.name.clone(), Kind::Use, alias.span))
                    .or_else(|| {
                        definition.node.path.node.segments.last().map(|segment| {
                            (segment.node.name.node.name.clone(), Kind::Use, segment.span)
                        })
                    }),
                _ => None,
            }
            .map(|(name, kind, span)| SyntaxSymbol {
                name,
                kind,
                start: span.start,
                end: span.end,
            })
        })
        .collect()
}

fn bump_entry_file_revision(db: &mut beskid_queries::BeskidDatabase, resolved: &ResolvedInput) {
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
    let mut resolved = resolve_input(Some(&path.to_path_buf()), None, None, None, false, false)
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

async fn build_syntax_facts(state: &RwLock<State>, uri: &Uri, text: &str) -> SyntaxFacts {
    wait_for_initial_scan(state).await;
    let documentation = if is_manifest_uri(uri) {
        Vec::new()
    } else {
        syntax_documentation_facts_for_source(uri.as_str(), text)
    };
    if is_manifest_uri(uri) {
        let diagnostics =
            collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts {
            documentation,
            diagnostics,
            ..SyntaxFacts::default()
        };
    }
    let Some(path) = uri_to_path(uri) else {
        let diagnostics =
            collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts {
            documentation,
            diagnostics,
            ..SyntaxFacts::default()
        };
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        let diagnostics =
            collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts {
            documentation,
            diagnostics,
            ..SyntaxFacts::default()
        };
    };
    let mut facts = with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = session.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path, text.to_string());
        let options = PrepareOptions::default();
        match typed_entry_state_with_db(db, &resolved, &options, None) {
            Ok(entry_state) => syntax_facts_for_entry(db, &resolved, &entry_state),
            Err(_) => SyntaxFacts::default(),
        }
    })
    .await;
    facts.diagnostics =
        collect_syntax_diagnostics_for_state(state, uri, text, Some(&session)).await;
    facts.documentation = documentation;
    facts
}

fn document_from_syntax_facts(version: i32, text: String, syntax_facts: SyntaxFacts) -> Document {
    Document {
        version,
        text,
        syntax_definitions: syntax_facts.definitions,
        syntax_hovers: syntax_facts.hovers,
        syntax_symbols: syntax_facts.symbols,
        syntax_completion: syntax_facts.completion,
        syntax_inlay_hints: syntax_facts.inlay_hints,
        syntax_documentation: syntax_facts.documentation,
        syntax_diagnostics: syntax_facts.diagnostics,
    }
}

fn apply_syntax_facts(doc: &mut Document, syntax_facts: SyntaxFacts) {
    doc.syntax_definitions = syntax_facts.definitions;
    doc.syntax_hovers = syntax_facts.hovers;
    doc.syntax_symbols = syntax_facts.symbols;
    doc.syntax_completion = syntax_facts.completion;
    doc.syntax_inlay_hints = syntax_facts.inlay_hints;
    doc.syntax_documentation = syntax_facts.documentation;
    doc.syntax_diagnostics = syntax_facts.diagnostics;
}

/// Build a [`Document`] for `uri` with generation-bound syntax facts for the buffer text.
pub async fn build_document(
    state: &RwLock<State>,
    uri: &Uri,
    version: i32,
    text: String,
) -> Document {
    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    document_from_syntax_facts(version, text, syntax_facts)
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

    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    let mut write = state.write().await;
    if let Some(doc) = write.docs.get_mut(uri)
        && doc.text == text
    {
        apply_syntax_facts(doc, syntax_facts);
    } else if let Some(doc) = write.workspace_index.get_mut(uri)
        && doc.text == text
    {
        apply_syntax_facts(doc, syntax_facts);
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
        write
            .typed_prepare_schedule_revision
            .insert(uri.clone(), next);
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

/// Upsert an open document, respecting monotonic versions.
///
/// Same-text updates still rebuild generation-bound syntax facts so hard invalidation cannot
/// leave a stale empty or orphaned fact set behind a text-hash fast path.
///
/// Returns `false` when `version` is stale relative to the buffered document (no mutation).
pub async fn set_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) -> bool {
    {
        let mut write_state = state.write().await;
        write_state.workspace_index.remove(&uri);
        if let Some(existing) = write_state.docs.get(&uri)
            && version < existing.version
        {
            return false;
        }
    }

    touch_entry_file_revision_for_uri(state, &uri, &text).await;
    let syntax_facts = build_syntax_facts(state, &uri, &text).await;

    let mut write_state = state.write().await;
    if let Some(existing) = write_state.docs.get(&uri)
        && version < existing.version
    {
        return false;
    }
    write_state
        .docs
        .insert(uri, document_from_syntax_facts(version, text, syntax_facts));
    true
}

/// Drop an open buffer after `didClose` (disk hydration may repopulate the workspace index).
pub async fn remove_document(state: &RwLock<State>, uri: &Uri) {
    let mut write = state.write().await;
    write.docs.remove(uri);
    write.typed_prepare_schedule_revision.remove(uri);
}

/// Rebuild generation-bound syntax facts (including diagnostics) for open `.bd` buffers after
/// compilation context invalidation.
pub async fn rebuild_open_document_syntax_facts(state: &RwLock<State>) {
    let entries: Vec<(Uri, String)> = {
        let read = state.read().await;
        read.docs
            .iter()
            .filter(|(uri, _)| !is_manifest_uri(uri))
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect()
    };

    for (uri, text) in entries {
        let syntax_facts = build_syntax_facts(state, &uri, &text).await;
        let mut write = state.write().await;
        if let Some(doc) = write.docs.get_mut(&uri)
            && doc.text == text
        {
            apply_syntax_facts(doc, syntax_facts);
        }
    }
}

/// Refresh generation-bound diagnostic facts for the open buffer or workspace snapshot and push
/// to the client. Never reads `Document.analysis` / HIR snapshots.
pub async fn publish_diagnostics_for_uri(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let snapshot = {
        let state = state.read().await;
        state.document_union(uri)
    };

    let Some(doc) = snapshot else {
        return;
    };

    let text = doc.text.clone();
    let version = doc.version;
    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    let diagnostics = lsp_diagnostics_from_syntax(&text, &syntax_facts.diagnostics);
    {
        let mut write = state.write().await;
        if let Some(open) = write.docs.get_mut(uri)
            && open.text == text
        {
            apply_syntax_facts(open, syntax_facts);
        } else if let Some(indexed) = write.workspace_index.get_mut(uri)
            && indexed.text == text
        {
            apply_syntax_facts(indexed, syntax_facts);
        }
    }
    client
        .publish_diagnostics(uri.clone(), diagnostics, Some(version))
        .await;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::Uri;

    use super::{rebuild_open_document_syntax_facts, set_document};
    use crate::session::project_context::invalidate_compilation_cache;
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
    async fn hard_invalidation_clears_syntax_facts_until_rebuild() {
        let file_uri = uri();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        set_document(&state, file_uri.clone(), 1, source()).await;
        {
            let read = state.read().await;
            let doc = read.docs.get(&file_uri).expect("document exists");
            assert!(
                doc.syntax_documentation
                    .iter()
                    .any(|fact| fact.name == "Main"),
                "precondition: documentation facts bound"
            );
        }

        // Non-cold configured root so invalidate clears bound facts.
        {
            let mut write = state.write().await;
            write.configured_project_root = Some(std::path::PathBuf::from("/tmp/cyb78"));
        }

        invalidate_compilation_cache(&state).await;
        {
            let read = state.read().await;
            let doc = read.docs.get(&file_uri).expect("document exists");
            assert!(
                doc.syntax_documentation.is_empty()
                    && doc.syntax_diagnostics.is_empty()
                    && doc.syntax_completion.is_none(),
                "hard invalidation must fail closed without a shape-version cache"
            );
        }

        rebuild_open_document_syntax_facts(&state).await;
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert!(
            doc.syntax_documentation
                .iter()
                .any(|fact| fact.name == "Main"),
            "rebuild must rebind documentation facts to the current buffer"
        );
    }

    #[tokio::test]
    async fn set_document_refreshes_documentation_facts_for_new_buffer_text() {
        let file_uri = uri();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        set_document(&state, file_uri.clone(), 1, "i32 Old() { return 0; }".into()).await;
        {
            let read = state.read().await;
            let doc = read.docs.get(&file_uri).expect("document exists");
            assert!(doc.syntax_documentation.iter().any(|fact| fact.name == "Old"));
            assert!(!doc.syntax_documentation.iter().any(|fact| fact.name == "Current"));
        }
        set_document(
            &state,
            file_uri.clone(),
            2,
            "i32 Current() { return 0; }".into(),
        )
        .await;
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert!(
            doc.syntax_documentation
                .iter()
                .any(|fact| fact.name == "Current"),
            "refresh must replace stale documentation facts"
        );
        assert!(!doc.syntax_documentation.iter().any(|fact| fact.name == "Old"));
    }

    #[tokio::test]
    async fn set_document_binds_syntax_diagnostics_without_analysis_snapshot() {
        let file_uri = uri();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        set_document(&state, file_uri.clone(), 1, source()).await;
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        // Valid buffer: structural/prepare facts may be empty, but the field must be owned
        // by the Document revision (no Document.analysis snapshot).
        let _ = &doc.syntax_diagnostics;
        assert!(
            doc.syntax_diagnostics
                .iter()
                .all(|diag| diag.code.as_deref() != Some("E1709")),
            "refresh must not attach orphaned composition diagnostics"
        );
    }

    #[tokio::test]
    async fn set_document_rebuilds_same_text_after_cleared_facts() {
        let file_uri = uri();
        let text = source();
        let mut state = State::default();
        state.docs.insert(
            file_uri.clone(),
            Document {
                version: 1,
                text: text.clone(),
                syntax_definitions: Vec::new(),
                syntax_hovers: Vec::new(),
                syntax_symbols: Vec::new(),
                syntax_completion: None,
                syntax_inlay_hints: Vec::new(),
                syntax_documentation: Vec::new(),
                syntax_diagnostics: Vec::new(),
            },
        );
        state.mark_initial_scan_complete();
        let state = tokio::sync::RwLock::new(state);
        set_document(&state, file_uri.clone(), 2, text).await;
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert_eq!(doc.version, 2);
        assert!(
            doc.syntax_documentation
                .iter()
                .any(|fact| fact.name == "Main"),
            "same-text upsert must rebuild facts after a cleared snapshot-free document"
        );
    }
}
