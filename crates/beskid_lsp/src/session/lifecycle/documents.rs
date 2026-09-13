use std::sync::Arc;

use beskid_analysis::services::{DependencyTypingPolicy, FrontEndOptions, PrepareOptions};
use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::{
    diagnostics::{
        collect_syntax_diagnostics, prepare_project_diagnostics_from_assembled, prepare_project_syntax_facts,
    },
    manifest_uri::{is_manifest_uri, is_standalone_bsol_uri},
    session::{
        db_access::{document_update_gate, with_compilation_db_for_project},
        diagnostics_bridge::collect_syntax_diagnostics_for_state,
        documentation_facts::syntax_documentation_facts_for_source,
        imports::RecoverableCompletionSyntax,
        startup::wait_for_initial_scan,
        store::{Document, State, SyntaxCompletion},
    },
    workspace_scan::uri_to_path,
};

#[derive(Clone, Copy)]
enum StartupFallback {
    WaitForScan,
    StructuralOnly,
}

async fn fallback_diagnostics(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    compilation_context: Option<&beskid_analysis::CompilationContext>,
    fallback: StartupFallback,
) -> (Vec<crate::session::store::SyntaxDiagnostic>, Vec<beskid_analysis::SyntaxFix>) {
    match fallback {
        StartupFallback::WaitForScan => {
            collect_syntax_diagnostics_for_state(state, uri, text, compilation_context).await
        }
        // The first workspace scan owns the startup barrier. If a source cannot
        // be prepared as a project, using the normal bridge here would wait for
        // this same scan to finish and deadlock the server before it can publish
        // even structural diagnostics.
        StartupFallback::StructuralOnly => collect_syntax_diagnostics(None, uri, text, compilation_context),
    }
}

pub(super) async fn build_full_diagnostic_facts(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
) -> (Vec<crate::session::store::SyntaxDiagnostic>, Vec<beskid_analysis::SyntaxFix>) {
    wait_for_initial_scan(state).await;
    if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        return collect_syntax_diagnostics_for_state(state, uri, text, None).await;
    }
    let Some(path) = uri_to_path(uri) else {
        return collect_syntax_diagnostics_for_state(state, uri, text, None).await;
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        return collect_syntax_diagnostics_for_state(state, uri, text, None).await;
    };
    let Some(plan) = session.compile_plan.as_ref() else {
        return collect_syntax_diagnostics_for_state(state, uri, text, None).await;
    };
    let buffered_sources = {
        let read = state.read().await;
        read.workspace_index
            .iter()
            .chain(read.docs.iter())
            .filter_map(|(buffer_uri, document)| {
                uri_to_path(buffer_uri).map(|buffer_path| (buffer_path, document.text.clone()))
            })
            .collect::<Vec<_>>()
    };
    let project_root = plan.project_root.clone();
    let options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        dependency_typing: DependencyTypingPolicy::FullClosure,
    };
    let assembled = with_compilation_db_for_project(state, &project_root, |db| {
        beskid_queries::invalidate_entry_sessions(&project_root);
        for (buffer_path, buffer_text) in buffered_sources {
            db.ensure_file_text(buffer_path, buffer_text);
        }
        db.ensure_file_text(path, text.to_string());
        beskid_queries::assemble_resolved_input_with_db(db, &resolved, &options).ok()
    })
    .await;
    let Some(assembled) = assembled else {
        return collect_syntax_diagnostics_for_state(state, uri, text, None).await;
    };
    let prepared = tokio::task::spawn_blocking(move || {
        prepare_project_diagnostics_from_assembled(&assembled, DependencyTypingPolicy::FullClosure)
            .map(|prepared| (prepared.diagnostics, prepared.fixes))
            .ok()
    })
    .await
    .ok()
    .flatten();
    match prepared {
        Some(facts) => facts,
        None => collect_syntax_diagnostics_for_state(state, uri, text, None).await,
    }
}

use super::{
    facts::{SyntaxFacts, bsol_semantic_token_candidates, syntax_facts_for_assembly, syntax_symbols_for_program},
    revisions_resolution::resolved_input_for_path,
};

pub(super) async fn build_syntax_facts_with_policy(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    dependency_typing: DependencyTypingPolicy,
    current_document_only: bool,
) -> SyntaxFacts {
    wait_for_initial_scan(state).await;
    build_syntax_facts_with_policy_after_startup(
        state,
        uri,
        text,
        dependency_typing,
        current_document_only,
        StartupFallback::WaitForScan,
    )
    .await
}

async fn build_syntax_facts_with_policy_after_startup(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    dependency_typing: DependencyTypingPolicy,
    current_document_only: bool,
    fallback: StartupFallback,
) -> SyntaxFacts {
    let documentation = if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        Vec::new()
    } else {
        syntax_documentation_facts_for_source(uri.as_str(), text)
    };
    // Outline symbols belong to the current buffer's recoverable syntax, not to
    // the last successfully typed project closure. Keep this independent of
    // resolution so an incomplete expression cannot erase or stale the outline.
    let current_symbols = if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        Vec::new()
    } else {
        beskid_analysis::services::parse_program_with_source_name(uri.as_str(), text)
            .map(|program| syntax_symbols_for_program(&program))
            .unwrap_or_default()
    };
    if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, None, fallback).await;
        return SyntaxFacts {
            documentation,
            diagnostics,
            fixes,
            bsol_semantic_token_candidates: bsol_semantic_token_candidates(text),
            ..SyntaxFacts::default()
        };
    }
    let Some(path) = uri_to_path(uri) else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, None, fallback).await;
        return SyntaxFacts { documentation, symbols: current_symbols, diagnostics, fixes, ..SyntaxFacts::default() };
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, None, fallback).await;
        return SyntaxFacts { documentation, symbols: current_symbols, diagnostics, fixes, ..SyntaxFacts::default() };
    };
    if current_document_only {
        let read = state.read().await;
        if read.docs.get(uri).or_else(|| read.workspace_index.get(uri)).is_none_or(|document| document.text != text) {
            return SyntaxFacts { documentation, symbols: current_symbols, ..SyntaxFacts::default() };
        }
    }
    let Some(plan) = session.compile_plan.as_ref() else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, Some(&session), fallback).await;
        return SyntaxFacts { documentation, symbols: current_symbols, diagnostics, fixes, ..SyntaxFacts::default() };
    };
    let buffered_sources = {
        let read = state.read().await;
        read.workspace_index
            .iter()
            .chain(read.docs.iter())
            .filter_map(|(buffer_uri, document)| {
                uri_to_path(buffer_uri).map(|buffer_path| (buffer_path, document.text.clone()))
            })
            .collect::<Vec<_>>()
    };
    let project_root = plan.project_root.clone();
    let resolved_for_prepare = resolved.clone();
    let path_for_prepare = path.clone();
    let text_for_prepare = text.to_string();
    let prepared_facts = with_compilation_db_for_project(state, &project_root, |db| {
        beskid_queries::invalidate_entry_sessions(&project_root);
        for (buffer_path, buffer_text) in buffered_sources {
            db.ensure_file_text(buffer_path, buffer_text);
        }
        db.ensure_file_text(path_for_prepare.clone(), text_for_prepare);
        let entry_key = beskid_queries::session_fingerprint(&resolved_for_prepare)
            .map(|fingerprint| beskid_queries::fingerprint_key(&fingerprint))
            .unwrap_or_else(|| path_for_prepare.display().to_string());

        let prepared = if dependency_typing == DependencyTypingPolicy::EntryOnly {
            beskid_queries::bump_file_revision(db, &entry_key);
            let options = PrepareOptions {
                front_end: FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
                dependency_typing,
            };
            let prepared = prepare_project_syntax_facts(db, &resolved_for_prepare, dependency_typing).ok()?;
            // Resolve registry-backed identity immediately, without waiting for the
            // full executable dependency closure, against the same post-mod-rewrite
            // assembly that owns the generated syntax facts.
            let resolved_generation = resolved_for_prepare.with_assembly(prepared.assembly.clone());
            // Semantic resolution enriches the fast snapshot when the edit is
            // complete enough, but a transient type error must not discard the
            // recoverable import/member surface needed to finish that edit.
            let _ = beskid_queries::entry_resolution_with_db(db, &resolved_generation, &options);
            prepared
        } else {
            prepare_project_syntax_facts(db, &resolved_for_prepare, dependency_typing).ok()?
        };
        let mut facts = syntax_facts_for_assembly(db, &resolved_for_prepare, Arc::new(prepared.assembly));
        facts.diagnostics = prepared.diagnostics;
        facts.fixes = prepared.fixes;
        Some(facts)
    })
    .await;
    let mut facts = if let Some(facts) = prepared_facts {
        facts
    } else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, Some(&session), fallback).await;
        SyntaxFacts { diagnostics, fixes, ..SyntaxFacts::default() }
    };
    facts.documentation = documentation;
    facts.symbols = current_symbols;
    facts
}

pub(super) async fn build_syntax_facts(state: &RwLock<State>, uri: &Uri, text: &str) -> SyntaxFacts {
    build_syntax_facts_with_policy(state, uri, text, DependencyTypingPolicy::FullClosure, false).await
}

fn document_from_syntax_facts(version: i32, text: String, syntax_facts: SyntaxFacts) -> Document {
    Document {
        version,
        text,
        syntax_definitions: syntax_facts.definitions,
        syntax_hovers: syntax_facts.hovers,
        syntax_symbols: syntax_facts.symbols,
        bsol_semantic_token_candidates: syntax_facts.bsol_semantic_token_candidates,
        syntax_completion: syntax_facts.completion,
        syntax_inlay_hints: syntax_facts.inlay_hints,
        syntax_documentation: syntax_facts.documentation,
        syntax_diagnostics: syntax_facts.diagnostics,
        syntax_fixes: syntax_facts.fixes,
    }
}

pub(super) fn apply_syntax_facts(doc: &mut Document, syntax_facts: SyntaxFacts) {
    doc.syntax_definitions = syntax_facts.definitions;
    doc.syntax_hovers = syntax_facts.hovers;
    doc.syntax_symbols = syntax_facts.symbols;
    doc.bsol_semantic_token_candidates = syntax_facts.bsol_semantic_token_candidates;
    doc.syntax_completion = syntax_facts.completion;
    doc.syntax_inlay_hints = syntax_facts.inlay_hints;
    doc.syntax_documentation = syntax_facts.documentation;
    doc.syntax_diagnostics = syntax_facts.diagnostics;
    doc.syntax_fixes = syntax_facts.fixes;
}

/// Build a [`Document`] for `uri` with generation-bound syntax facts for the buffer text.
pub async fn build_document(state: &RwLock<State>, uri: &Uri, version: i32, text: String) -> Document {
    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    document_from_syntax_facts(version, text, syntax_facts)
}

/// Build a closed-file snapshot while the initial workspace scan is still in progress.
///
/// This bypasses only the startup barrier; type-checking and the generation-bound document
/// representation are otherwise identical to [`build_document`].
pub(crate) async fn build_initial_workspace_document(
    state: &RwLock<State>,
    uri: &Uri,
    version: i32,
    text: String,
) -> Document {
    let syntax_facts = build_syntax_facts_with_policy_after_startup(
        state,
        uri,
        &text,
        DependencyTypingPolicy::FullClosure,
        false,
        StartupFallback::StructuralOnly,
    )
    .await;
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

/// Upsert an open document, respecting monotonic versions.
///
/// Same-text updates still rebuild generation-bound syntax facts so hard invalidation cannot
/// leave a stale empty or orphaned fact set behind a text-hash fast path.
///
/// Returns `false` when `version` is stale relative to the buffered document (no mutation).
pub async fn set_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) -> bool {
    let update_gate = document_update_gate(state, &uri).await;
    let _update_guard = update_gate.lock().await;
    let prior_dependency_surface = {
        let mut write_state = state.write().await;
        if let Some(existing) = write_state.docs.get(&uri)
            && version < existing.version
        {
            return false;
        }
        let current_syntax = RecoverableCompletionSyntax::parse(&text);
        let prior_dependency_surface = write_state
            .docs
            .get(&uri)
            .or_else(|| write_state.workspace_index.get(&uri))
            .and_then(|document| document.syntax_completion.as_ref())
            .map(|completion| {
                completion
                    .dependency_surface
                    .iter()
                    .filter(|surface| {
                        current_syntax
                            .as_ref()
                            .is_some_and(|syntax| syntax.imports(surface.receiver.as_ref(), &surface.import_path))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        write_state.workspace_index.remove(&uri);
        prior_dependency_surface
    };

    // Keep editing responsive: type only the entry unit synchronously. The debounced
    // lifecycle pass upgrades this exact text generation to the full dependency closure.
    let mut syntax_facts =
        build_syntax_facts_with_policy(state, &uri, &text, DependencyTypingPolicy::EntryOnly, false).await;
    if !prior_dependency_surface.is_empty()
        && syntax_facts.completion.as_ref().is_none_or(|completion| completion.dependency_surface.is_empty())
    {
        let dependency_surface = prior_dependency_surface.into();
        match &mut syntax_facts.completion {
            Some(completion) => completion.dependency_surface = dependency_surface,
            None => {
                syntax_facts.completion = Some(SyntaxCompletion { entry_anchor: None, dependency_surface });
            }
        }
    }

    let mut write_state = state.write().await;
    if let Some(existing) = write_state.docs.get(&uri)
        && version < existing.version
    {
        return false;
    }
    write_state.docs.insert(uri, document_from_syntax_facts(version, text, syntax_facts));
    true
}

/// Drop an open buffer after `didClose` (disk hydration may repopulate the workspace index).
pub async fn remove_document(state: &RwLock<State>, uri: &Uri) {
    let update_gate = document_update_gate(state, uri).await;
    let _update_guard = update_gate.lock().await;
    let mut write = state.write().await;
    write.docs.remove(uri);
}

/// Rebuild generation-bound syntax facts (including diagnostics) for open `.bd` buffers after
/// compilation context invalidation.
pub async fn rebuild_open_document_syntax_facts(state: &RwLock<State>) {
    let entries: Vec<(Uri, i32, String)> = {
        let read = state.read().await;
        read.docs
            .iter()
            .filter(|(uri, _)| !is_manifest_uri(uri) && !is_standalone_bsol_uri(uri))
            .map(|(uri, doc)| (uri.clone(), doc.version, doc.text.clone()))
            .collect()
    };

    for (uri, version, text) in entries {
        let syntax_facts =
            build_syntax_facts_with_policy(state, &uri, &text, DependencyTypingPolicy::FullClosure, true).await;
        let update_gate = document_update_gate(state, &uri).await;
        let _update_guard = update_gate.lock().await;
        let mut write = state.write().await;
        if let Some(doc) = write.docs.get_mut(&uri)
            && doc.version == version
            && doc.text == text
        {
            apply_syntax_facts(doc, syntax_facts);
        }
    }
}
