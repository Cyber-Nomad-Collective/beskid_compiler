use std::sync::Arc;

use beskid_analysis::{
    projects::{CompilePlan, effective_roots_from_lockfile_checked},
    services::{FrontEndOptions, PrepareOptions},
};
use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::{
    diagnostics::{
        EditorPreparePass, collect_syntax_diagnostics, prepare_project_diagnostics_from_assembled,
        prepare_project_index, prepare_project_syntax_facts, project_preparation_failure, unit_diagnostic_names,
    },
    features::formatting::open_texts::open_text,
    manifest_uri::{is_manifest_uri, is_standalone_bsol_uri},
    session::{
        db_access::{
            Lane, buffered_sources, document_update_gate, run_blocking_section, run_in_project_db_with, with_index_db,
        },
        diagnostics_bridge::collect_syntax_diagnostics_for_state,
        documentation_facts::syntax_documentation_facts_for_source,
        imports::RecoverableCompletionSyntax,
        startup::wait_for_initial_scan,
        store::{Document, State, SyntaxCompletion, SyntaxDiagnostic, SyntaxDiagnosticSeverity},
    },
    workspace_scan::uri_to_path,
};

#[derive(Clone, Copy)]
enum StartupFallback {
    WaitForScan,
    StructuralOnly,
}

fn lockfile_replay_diagnostic(plan: &CompilePlan) -> Option<SyntaxDiagnostic> {
    effective_roots_from_lockfile_checked(plan, &plan.project_root.join("Project.lock")).err().map(|error| {
        SyntaxDiagnostic {
            start: 0,
            end: 0,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some(error.code().into()),
            message: error.to_string(),
            source: "beskid".into(),
            detail: Default::default(),
        }
    })
}

/// The lock replay diagnostic of a project manifest (`E3025` for a lock in an unsupported format,
/// a malformed or a stale lock). The source files of the project report it too; the manifest
/// publishes it as well, because the lock belongs to the project the `.bproj` declares.
fn manifest_lock_diagnostic(uri: &Uri) -> Option<SyntaxDiagnostic> {
    if !is_manifest_uri(uri) {
        return None;
    }
    let path = uri_to_path(uri)?;
    if path.extension()? != "bproj" {
        return None;
    }
    let plan = beskid_analysis::projects::build_compile_plan(&path, None).ok()?;
    lockfile_replay_diagnostic(&plan)
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
) -> Option<(Vec<crate::session::store::SyntaxDiagnostic>, Vec<beskid_analysis::SyntaxFix>)> {
    wait_for_initial_scan(state).await;
    if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        let (mut diagnostics, fixes) = collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        diagnostics.extend(manifest_lock_diagnostic(uri));
        return Some((diagnostics, fixes));
    }
    let Some(path) = uri_to_path(uri) else {
        return Some(collect_syntax_diagnostics_for_state(state, uri, text, None).await);
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        return Some(collect_syntax_diagnostics_for_state(state, uri, text, None).await);
    };
    let Some(plan) = session.compile_plan.as_ref() else {
        return Some(collect_syntax_diagnostics_for_state(state, uri, text, None).await);
    };
    if let Some(diagnostic) = lockfile_replay_diagnostic(plan) {
        return Some((vec![diagnostic], Vec::new()));
    }
    let interactive = state.read().await.docs.contains_key(uri);
    let project_root = plan.project_root.clone();
    let mod_host = state.read().await.mod_host.clone();
    let options = PrepareOptions {
        mod_host: beskid_analysis::mod_host::ModHost::NoMods,
        front_end: FrontEndOptions {
            assembly_recovery: beskid_analysis::projects::AssemblyRecoveryPolicy::EditorRetainRecovered,
            with_semantic_diagnostics: true,
            ..Default::default()
        },
        native_mod_adapter_sources: false,
    };
    // The client sent a newer text of this open document: its own publication follows.
    let newer_text_sent = || open_text(uri).is_some_and(|newest| newest != text);
    // Held from the assembly through the preparation: the assembled program is large.
    let permits = Arc::clone(&state.read().await.diagnostics_permits);
    let _permit = permits.acquire_owned().await.ok()?;
    if newer_text_sent() {
        return None;
    }
    let lane = if interactive { Lane::Interactive } else { Lane::ClosedFile };
    let assembled = run_in_project_db_with(state, Some(&project_root), lane, false, buffered_sources, |sources, db| {
        if newer_text_sent() {
            return None;
        }
        beskid_queries::invalidate_entry_sessions(&project_root);
        for (buffer_path, buffer_text) in sources {
            db.ensure_file_text(buffer_path, buffer_text);
        }
        db.ensure_file_text(path, text.to_string());
        Some(beskid_queries::assemble_resolved_input_with_db(db, &resolved, &options))
    })
    .await?;
    let assembled = match assembled {
        Ok(assembled) => assembled,
        Err(error) => return Some(preparation_failure_facts(uri, text, &error)),
    };
    if newer_text_sent() {
        return None;
    }
    let prepared = tokio::task::spawn_blocking(move || {
        prepare_project_diagnostics_from_assembled(&assembled, EditorPreparePass::Complete, &mod_host)
            .map(|prepared| (prepared.diagnostics, prepared.fixes))
    })
    .await
    .map_err(|error| anyhow::anyhow!("project diagnostics task failed: {error}"))
    .and_then(|prepared| prepared);
    Some(match prepared {
        Ok(facts) => facts,
        Err(error) => preparation_failure_facts(uri, text, &error),
    })
}

/// Structural facts for the current buffer plus a visible error fact naming why project
/// preparation failed. A failed preparation is never rerun through another path and never
/// silently degrades to structural-only facts.
fn preparation_failure_facts(
    uri: &Uri,
    text: &str,
    error: &anyhow::Error,
) -> (Vec<crate::session::store::SyntaxDiagnostic>, Vec<beskid_analysis::SyntaxFix>) {
    tracing::warn!(uri = %uri.as_str(), "project preparation failed: {error:#}");
    let (mut diagnostics, _fixes) = collect_syntax_diagnostics(None, uri, text, None);
    let failure = project_preparation_failure(uri, error, &diagnostics);
    diagnostics.extend(failure);
    (diagnostics, Vec::new())
}

use crate::session::attribute_facts::attribute_names_for_program;

use super::{
    facts::{
        SyntaxFacts, bsol_semantic_token_candidates, syntax_facts_for_assembly, syntax_symbols_for_program,
        typed_program_for_assembly, unit_syntax_facts,
    },
    revisions_resolution::resolved_input_for_path,
};

pub(super) async fn build_syntax_facts_with_policy(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    pass: EditorPreparePass,
    current_document_only: bool,
    newest_only: bool,
) -> SyntaxFacts {
    wait_for_initial_scan(state).await;
    build_syntax_facts_with_policy_after_startup(
        state,
        uri,
        text,
        pass,
        current_document_only,
        newest_only,
        StartupFallback::WaitForScan,
    )
    .await
}

/// Whether the client sent a newer text for `uri` than `text` (or closed it).
fn superseded(uri: &Uri, text: &str) -> bool {
    open_text(uri).as_deref() != Some(text)
}

async fn build_syntax_facts_with_policy_after_startup(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    pass: EditorPreparePass,
    current_document_only: bool,
    newest_only: bool,
    fallback: StartupFallback,
) -> SyntaxFacts {
    let newest = newest_only.then_some(text);
    let mut facts = syntax_facts_for_text(state, uri, text, pass, current_document_only, newest, fallback, &[]).await;
    if facts.superseded {
        return facts;
    }
    // While a member is being typed (`String.` or `point.` ending its line) the buffer does not
    // parse with the member access in it. Completion takes its facts from the same buffer with a
    // placeholder member written over the following whitespace (byte offsets unchanged); every
    // other fact (diagnostics, hovers, outline) stays the buffer's own. The repaired facts are
    // built last, so the registered generation is the one the completion anchor names.
    if !is_manifest_uri(uri)
        && !is_standalone_bsol_uri(uri)
        && let Some((repaired, offsets)) = crate::session::imports::completion_repaired_text(text)
    {
        let repaired = syntax_facts_for_text(state, uri, &repaired, pass, false, newest, fallback, &offsets).await;
        if repaired.superseded {
            return repaired;
        }
        if let Some(completion) = repaired.completion
            && completion.entry_anchor.is_some()
        {
            facts.completion = Some(completion);
        }
    }
    facts
}

async fn syntax_facts_for_text(
    state: &RwLock<State>,
    uri: &Uri,
    text: &str,
    pass: EditorPreparePass,
    current_document_only: bool,
    newest: Option<&str>,
    fallback: StartupFallback,
    member_offsets: &[usize],
) -> SyntaxFacts {
    // The recoverable parse of a broken buffer takes a while (the first time for a text; later
    // parses reuse the remembered repair), so it runs off the async workers.
    let (documentation, (current_symbols, current_attribute_names)) =
        if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
            (Vec::new(), (Vec::new(), Vec::new()))
        } else {
            run_blocking_section(|| {
                let documentation = syntax_documentation_facts_for_source(uri.as_str(), text);
                // Outline symbols belong to the current buffer's recoverable syntax, not to
                // the last successfully typed project closure. Keep this independent of
                // resolution so an incomplete expression cannot erase or stale the outline.
                let symbols = beskid_analysis::services::parse_program_with_source_name(uri.as_str(), text)
                    .map(|program| (syntax_symbols_for_program(&program), attribute_names_for_program(&program)))
                    .unwrap_or_default();
                (documentation, symbols)
            })
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
        return SyntaxFacts {
            documentation,
            symbols: current_symbols,
            attribute_names: current_attribute_names,
            diagnostics,
            fixes,
            ..SyntaxFacts::default()
        };
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, None, fallback).await;
        return SyntaxFacts {
            documentation,
            symbols: current_symbols,
            attribute_names: current_attribute_names,
            diagnostics,
            fixes,
            ..SyntaxFacts::default()
        };
    };
    if current_document_only {
        let read = state.read().await;
        if read.docs.get(uri).or_else(|| read.workspace_index.get(uri)).is_none_or(|document| document.text != text) {
            return SyntaxFacts {
                documentation,
                symbols: current_symbols,
                attribute_names: current_attribute_names,
                ..SyntaxFacts::default()
            };
        }
    }
    let Some(plan) = session.compile_plan.as_ref() else {
        let (diagnostics, fixes) = fallback_diagnostics(state, uri, text, Some(&session), fallback).await;
        return SyntaxFacts {
            documentation,
            symbols: current_symbols,
            attribute_names: current_attribute_names,
            diagnostics,
            fixes,
            ..SyntaxFacts::default()
        };
    };
    if let Some(diagnostic) = lockfile_replay_diagnostic(plan) {
        return SyntaxFacts {
            documentation,
            symbols: current_symbols,
            attribute_names: current_attribute_names,
            diagnostics: vec![diagnostic],
            ..SyntaxFacts::default()
        };
    }
    let interactive = pass == EditorPreparePass::Immediate || state.read().await.docs.contains_key(uri);
    let project_root = plan.project_root.clone();
    let mod_host = state.read().await.mod_host.clone();
    let resolved_for_prepare = resolved.clone();
    let path_for_prepare = path.clone();
    let text_for_prepare = text.to_string();
    // The completion repair of a buffer analyzes in the storage of the buffer's own analysis.
    let analysis = member_offsets.is_empty();
    let lane = if interactive { Lane::Interactive } else { Lane::ClosedFile };
    let prepared_facts =
        run_in_project_db_with(state, Some(&project_root), lane, analysis, buffered_sources, |sources, db| {
            // The wait for the project's database can be long; a newer text makes this work moot.
            if newest.is_some_and(|buffer| superseded(uri, buffer)) {
                return Ok(None);
            }
            beskid_queries::invalidate_entry_sessions(&project_root);
            for (buffer_path, buffer_text) in sources {
                db.ensure_file_text(buffer_path, buffer_text);
            }
            db.ensure_file_text(path_for_prepare.clone(), text_for_prepare);
            let entry_key = beskid_queries::session_fingerprint(&resolved_for_prepare)
                .map(|fingerprint| beskid_queries::fingerprint_key(&fingerprint))
                .unwrap_or_else(|| path_for_prepare.display().to_string());

            let prepared = if pass == EditorPreparePass::Immediate {
                beskid_queries::bump_file_revision(db, &entry_key);
                let options = PrepareOptions {
                    mod_host: beskid_analysis::mod_host::ModHost::NoMods,
                    front_end: FrontEndOptions {
                        assembly_recovery: beskid_analysis::projects::AssemblyRecoveryPolicy::EditorRetainRecovered,
                        with_semantic_diagnostics: false,
                        ..Default::default()
                    },
                    native_mod_adapter_sources: false,
                };
                let prepared = prepare_project_syntax_facts(db, &resolved_for_prepare, pass, &mod_host)?;
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
                prepare_project_syntax_facts(db, &resolved_for_prepare, pass, &mod_host)?
            };
            let mut facts = syntax_facts_for_assembly(db, &resolved_for_prepare, Arc::new(prepared.assembly));
            // Capture the members of each repaired member access as owned data while this generation
            // is the registered one.
            if let Some(completion) = facts.completion.as_mut()
                && let Some(anchor) = completion.entry_anchor
                && !member_offsets.is_empty()
            {
                completion.member_access = member_offsets
                    .iter()
                    .filter_map(|&offset| {
                        let context = beskid_queries::CompletionContext {
                            cursor: offset,
                            replacement_start: offset,
                            replacement_end: offset,
                        };
                        let candidates = beskid_queries::completion_candidates(db, anchor, context).ok().flatten()?;
                        (!candidates.is_empty()).then_some((offset, candidates))
                    })
                    .collect::<Vec<_>>()
                    .into();
            }
            facts.diagnostics = prepared.diagnostics;
            facts.fixes = prepared.fixes;
            anyhow::Ok(Some(facts))
        })
        .await;
    let mut facts = match prepared_facts {
        Ok(Some(facts)) => facts,
        Ok(None) => return SyntaxFacts { superseded: true, ..SyntaxFacts::default() },
        Err(error) => {
            let (diagnostics, fixes) = preparation_failure_facts(uri, text, &error);
            SyntaxFacts { diagnostics, fixes, ..SyntaxFacts::default() }
        }
    };
    facts.documentation = documentation;
    facts.symbols = current_symbols;
    facts.attribute_names = current_attribute_names;
    facts
}

pub(super) async fn build_syntax_facts(state: &RwLock<State>, uri: &Uri, text: &str) -> SyntaxFacts {
    build_syntax_facts_with_policy(state, uri, text, EditorPreparePass::Complete, false, false).await
}

fn document_from_syntax_facts(version: i32, text: String, syntax_facts: SyntaxFacts) -> Document {
    Document {
        version,
        text,
        syntax_definitions: syntax_facts.definitions,
        syntax_hovers: syntax_facts.hovers,
        syntax_symbols: syntax_facts.symbols,
        bsol_semantic_token_candidates: syntax_facts.bsol_semantic_token_candidates,
        syntax_attribute_names: syntax_facts.attribute_names,
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
    doc.syntax_attribute_names = syntax_facts.attribute_names;
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
        EditorPreparePass::Complete,
        false,
        false,
        StartupFallback::StructuralOnly,
    )
    .await;
    document_from_syntax_facts(version, text, syntax_facts)
}

/// Documents for the closed files of one project from one typing of the whole project.
///
/// The workspace index used to prepare and type the project once per closed file, which made
/// the index of a large project take hours. Here every source under the project's roots is
/// assembled and typed once, and each file's facts and diagnostics come from that typed program.
/// A project whose preparation fails gives every file that failure, as preparing the file alone
/// would. Files the preparation does not cover are missing from the result, and `Err` names why
/// a project could not be indexed as a whole; the caller analyzes those files one by one.
pub(crate) async fn index_project_documents(
    state: &RwLock<State>,
    files: &[(Uri, std::path::PathBuf, String)],
) -> Result<std::collections::HashMap<Uri, Document>, String> {
    let Some((_, first_path, first_text)) = files.first() else {
        return Ok(Default::default());
    };
    let Some((resolved, session)) = resolved_input_for_path(state, first_path, first_text).await else {
        return Err("no project resolves the file".into());
    };
    // The target's entry file is the assembly entry when it is closed: its entry point is used.
    let entry = session
        .compile_plan
        .as_ref()
        .and_then(|plan| plan.target.entry.as_deref())
        .and_then(|entry| files.iter().find(|(_, path, _)| path.ends_with(entry) && path != first_path));
    let (resolved, session) = match entry {
        Some((_, path, text)) => resolved_input_for_path(state, path, text)
            .await
            .ok_or_else(|| "no project resolves the entry file".to_string())?,
        None => (resolved, session),
    };
    let Some(plan) = session.compile_plan.as_ref() else {
        return Err("the project has no compile plan".into());
    };
    // A lock that cannot be replayed is reported file by file.
    if let Some(diagnostic) = lockfile_replay_diagnostic(plan) {
        return Err(diagnostic.message);
    }
    let wanted: std::collections::HashMap<std::path::PathBuf, (&Uri, &str)> = files
        .iter()
        .map(|(uri, path, text)| (path.canonicalize().unwrap_or_else(|_| path.clone()), (uri, text.as_str())))
        .collect();
    let project_root = plan.project_root.clone();
    let mod_host = state.read().await.mod_host.clone();
    let derived = with_index_db(state, &project_root, |sources, db| {
        beskid_queries::invalidate_entry_sessions(&project_root);
        for (buffer_path, buffer_text) in sources {
            db.ensure_file_text(buffer_path, buffer_text);
        }
        for (path, (_, text)) in &wanted {
            db.ensure_file_text(path.clone(), (*text).to_string());
        }
        let prepared = match prepare_project_index(db, &resolved, &mod_host) {
            Ok(prepared) => prepared,
            // A project-level failure (a Mod without an artifact, an unreadable dependency) is
            // what preparing any one of its files would report: every file gets that fact
            // instead of repeating the failing preparation per file.
            Err(error) => {
                return Ok(wanted
                    .values()
                    .map(|(uri, text)| {
                        let (diagnostics, fixes) = preparation_failure_facts(uri, text, &error);
                        ((*uri).clone(), text.to_string(), SyntaxFacts { diagnostics, fixes, ..SyntaxFacts::default() })
                    })
                    .collect());
            }
        };
        let assembly = Arc::new(prepared.assembly);
        let typed = typed_program_for_assembly(db, &resolved, Arc::clone(&assembly))
            .ok_or_else(|| "the project's program could not be typed".to_string())?;
        let mut derived = Vec::new();
        for unit in assembly.units.iter() {
            let canonical = unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone());
            let Some((uri, text)) = wanted.get(&canonical) else {
                continue;
            };
            // Facts are offsets into the text the unit was assembled from.
            if unit.source != *text {
                continue;
            }
            let names = unit_diagnostic_names(unit);
            let mut facts = unit_syntax_facts(db, Some(&typed), &assembly, &unit.path, &unit.path, text);
            facts.diagnostics = prepared
                .diagnostics
                .iter()
                .filter(|(name, _)| names.contains(name))
                .map(|(_, diagnostic)| diagnostic.clone())
                .collect();
            derived.push(((*uri).clone(), text.to_string(), facts));
        }
        Ok::<_, String>(derived)
    })
    .await?;
    Ok(derived
        .into_iter()
        .map(|(uri, text, mut facts)| {
            facts.documentation = syntax_documentation_facts_for_source(uri.as_str(), &text);
            let (symbols, attribute_names) =
                beskid_analysis::services::parse_program_with_source_name(uri.as_str(), &text)
                    .map(|program| (syntax_symbols_for_program(&program), attribute_names_for_program(&program)))
                    .unwrap_or_default();
            facts.symbols = symbols;
            facts.attribute_names = attribute_names;
            let document = document_from_syntax_facts(0, text, facts);
            (uri, document)
        })
        .collect())
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
#[cfg(test)]
pub async fn set_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) -> bool {
    store_document(state, uri, version, text, false).await
}

/// Analyze the text the client sent for an open document and store its facts, unless the client
/// has sent a newer text or closed the document by then (the newer text's own analysis follows).
/// Requests that need the newest facts wait for this (see `protocol::request`).
pub async fn analyze_open_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) -> bool {
    store_document(state, uri, version, text, true).await
}

async fn store_document(state: &RwLock<State>, uri: Uri, version: i32, text: String, newest_only: bool) -> bool {
    let _span = crate::trace::Span::start("analyze", || format!("uri={} version={version}", uri.as_str()));
    let update_gate = document_update_gate(state, &uri).await;
    let _update_guard = update_gate.lock().await;
    if newest_only && superseded(&uri, &text) {
        return false;
    }
    // The recoverable parse of a broken buffer can take a while; it runs before the state
    // lock is taken, so requests on other documents never wait for it.
    let current_syntax = run_blocking_section(|| RecoverableCompletionSyntax::parse(&text));
    let (prior_dependency_surface, prior_attributes) = {
        let mut write_state = state.write().await;
        if let Some(existing) = write_state.docs.get(&uri)
            && version < existing.version
        {
            return false;
        }
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
        // The attribute catalogue is owned data from the last typed generation; keep it while
        // an incomplete edit (such as a freshly typed `[`) cannot be typed.
        let prior_attributes = write_state
            .docs
            .get(&uri)
            .or_else(|| write_state.workspace_index.get(&uri))
            .and_then(|document| document.syntax_completion.as_ref())
            .map(|completion| completion.attributes.clone())
            .unwrap_or_else(|| std::sync::Arc::from([]));
        write_state.workspace_index.remove(&uri);
        (prior_dependency_surface, prior_attributes)
    };

    // Keep editing responsive: type only the entry unit synchronously. The debounced
    // lifecycle pass upgrades this exact text generation to the full dependency closure.
    let mut syntax_facts =
        build_syntax_facts_with_policy(state, &uri, &text, EditorPreparePass::Immediate, false, newest_only).await;
    if syntax_facts.superseded || (newest_only && superseded(&uri, &text)) {
        return false;
    }
    if !prior_dependency_surface.is_empty()
        && syntax_facts.completion.as_ref().is_none_or(|completion| completion.dependency_surface.is_empty())
    {
        let dependency_surface = prior_dependency_surface.into();
        match &mut syntax_facts.completion {
            Some(completion) => completion.dependency_surface = dependency_surface,
            None => {
                syntax_facts.completion = Some(SyntaxCompletion {
                    entry_anchor: None,
                    dependency_surface,
                    attributes: std::sync::Arc::from([]),
                    member_access: std::sync::Arc::from([]),
                });
            }
        }
    }
    if !prior_attributes.is_empty() {
        match &mut syntax_facts.completion {
            Some(completion) if completion.attributes.is_empty() => completion.attributes = prior_attributes,
            Some(_) => {}
            None => {
                syntax_facts.completion = Some(SyntaxCompletion {
                    entry_anchor: None,
                    dependency_surface: std::sync::Arc::from([]),
                    attributes: prior_attributes,
                    member_access: std::sync::Arc::from([]),
                });
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
    write_state.facts_stored.notify_waiters();
    true
}

/// Drop an open buffer after `didClose` (disk hydration may repopulate the workspace index).
/// This does not wait for a running analysis of the buffer: that analysis sees the document
/// closed and does not store its facts.
pub async fn remove_document(state: &RwLock<State>, uri: &Uri) {
    let mut write = state.write().await;
    write.docs.remove(uri);
    write.facts_stored.notify_waiters();
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
            build_syntax_facts_with_policy(state, &uri, &text, EditorPreparePass::Complete, true, false).await;
        let update_gate = document_update_gate(state, &uri).await;
        let _update_guard = update_gate.lock().await;
        let mut write = state.write().await;
        if let Some(doc) = write.docs.get_mut(&uri)
            && doc.version == version
            && doc.text == text
        {
            apply_syntax_facts(doc, syntax_facts);
        }
        write.facts_stored.notify_waiters();
    }
}

#[cfg(test)]
mod portable_lock_diagnostic_tests {
    use std::{fs, str::FromStr};

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn lsp_document_reports_unsupported_malformed_and_stale_locks() {
        let temp = TempDir::new().expect("temp project");
        let root = temp.path();
        fs::create_dir_all(root.join("Src")).expect("source directory");
        let manifest_path = root.join("App.bproj");
        fs::write(
            &manifest_path,
            r#"App {
  name = "App"
  version = "0.1.0"
}

target "App" {
  kind = App
  entry = "Main.bd"
}
"#,
        )
        .expect("manifest");
        let source = "i32 Main() { return 0; }\n";
        let source_path = root.join("Src/Main.bd");
        fs::write(&source_path, source).expect("entry");
        let lock_path = root.join("Project.lock");
        let uri = Uri::from_str(&crate::workspace_scan::path_to_uri_string(&source_path)).expect("URI");
        let state = RwLock::new(State::default());

        let unsupported = "# Project.lock v1\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n";
        fs::write(&lock_path, unsupported).expect("unsupported lock");
        let reports_unsupported = |d: &SyntaxDiagnostic| {
            d.code.as_deref() == Some("E3025")
                && d.message.contains("unsupported format")
                && d.message.contains("beskid dev project lock")
        };
        let doc = build_initial_workspace_document(&state, &uri, 1, source.to_string()).await;
        assert!(doc.syntax_diagnostics.iter().any(reports_unsupported), "diagnostics: {:?}", doc.syntax_diagnostics);
        state.read().await.mark_initial_scan_complete();
        let (full_diagnostics, _) = build_full_diagnostic_facts(&state, &uri, source).await.expect("no newer text");
        assert!(full_diagnostics.iter().any(reports_unsupported), "diagnostics: {full_diagnostics:?}");
        // The manifest publishes it too.
        let manifest_uri =
            Uri::from_str(&crate::workspace_scan::path_to_uri_string(&manifest_path)).expect("manifest URI");
        let manifest_text = fs::read_to_string(&manifest_path).expect("manifest text");
        let (manifest_diagnostics, _) =
            build_full_diagnostic_facts(&state, &manifest_uri, &manifest_text).await.expect("manifest facts");
        assert!(manifest_diagnostics.iter().any(reports_unsupported), "manifest diagnostics: {manifest_diagnostics:?}");

        fs::write(&lock_path, "# Project.lock v3\nnot-a-lock\n").expect("malformed lock");
        let doc = build_initial_workspace_document(&state, &uri, 2, source.to_string()).await;
        assert!(doc.syntax_diagnostics.iter().any(|d| d.message.contains("lockfile")));

        fs::remove_file(&lock_path).expect("remove malformed fixture lock before generating a valid lock");
        let plan = beskid_analysis::projects::build_compile_plan(&manifest_path, None).expect("plan");
        beskid_analysis::projects::prepare_project_workspace_with_options(
            &plan,
            beskid_analysis::projects::WorkspacePrepareOptions { refresh_lock: true, ..Default::default() },
            None,
        )
        .expect("generate the lock");
        let valid = fs::read_to_string(&lock_path).expect("lock");
        fs::write(&lock_path, valid.replace("project_name=App", "project_name=Other")).expect("stale lock");
        let doc = build_initial_workspace_document(&state, &uri, 3, source.to_string()).await;
        let pin_free = !valid.lines().any(|line| line.starts_with("- ") && !line.contains(";source=corelib;"));
        // A copied lock that pins only Corelib is regenerated by the CLI without a word, so the
        // editor does not report it either; a lock with a user pin is reported.
        assert_eq!(
            doc.syntax_diagnostics.iter().any(|d| d.message.contains("different project")),
            !pin_free,
            "{:?}",
            doc.syntax_diagnostics
        );
    }
}
