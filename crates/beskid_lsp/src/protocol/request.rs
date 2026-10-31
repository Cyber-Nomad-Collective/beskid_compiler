//! The facts a document request answers from, and how long it waits for them.
//!
//! An open document is analyzed in the background after each edit. A request never waits for
//! more than a short, fixed budget: every `didChange` starts a new analysis, so a wait for "the
//! facts of the newest text" moves with each edit of a burst. Past the budget a request answers
//! in one of two ways:
//!
//! - A request whose answer is safe when it is not current (hover, definition, references,
//!   signature help, semantic tokens, inlay hints, completion) answers from the last analyzed
//!   facts carried over the edit ([`Document::carried_over`]).
//! - A request that produces edits (rename, prepare rename, code actions) never answers from
//!   facts of an older text. Past [`EDIT_FACTS_WAIT`] it fails with `ContentModified`
//!   (LSP 3.17 `-32801`), and it fails the same way when the client sends an edit while it
//!   computes its answer ([`CurrentDocument::still_current`]).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::features::formatting::open_texts::{OpenMatch, match_open_text, open_text, open_version};
use crate::session::store::{Document, State};
use crate::workspace_scan::uri_to_path;

/// Longest a request whose answer is safe when it is not current (references, signature help,
/// semantic tokens, inlay hints) waits for the analysis of the newest text before it answers
/// from the last facts carried over the edit.
pub const STALE_SAFE_FACTS_WAIT: Duration = Duration::from_millis(100);

/// Longest an edit-producing request (rename, prepare rename, code actions) waits for the
/// analysis of the newest text before it fails with `ContentModified`.
pub const EDIT_FACTS_WAIT: Duration = Duration::from_millis(500);

/// Longest a code-action request waits for its actions once it has the facts of the newest
/// text. Past it the answer holds the actions computed by then (diagnostic quick fixes first,
/// then the source actions such as "Format document"), and the rest are not computed.
pub const CODE_ACTION_COMPUTE_WAIT: Duration = Duration::from_millis(500);

/// Longest a background refresh task waits for the facts of the newest text before it asks the
/// client to refresh anyway (the client then gets the best facts stored by that time).
pub const REFRESH_FACTS_WAIT: Duration = Duration::from_secs(30);

/// The facts of the newest text of a document, with the open version they belong to.
#[derive(Debug, Clone)]
pub struct CurrentDocument {
    pub document: Document,
    /// The client's version of the text, `None` for a closed document (disk text).
    pub open_version: Option<i32>,
}

impl CurrentDocument {
    /// `false` when the client sent an edit (or closed the document) after these facts were
    /// taken: an answer computed from them would edit a text the client no longer has.
    pub fn still_current(&self, uri: &Uri) -> bool {
        open_version(uri) == self.open_version
    }
}

/// The facts a request found within its budget.
#[derive(Debug)]
pub enum RequestFacts {
    /// The facts of the newest text.
    Current(CurrentDocument),
    /// The last analyzed facts, carried over to the newest text (its analysis is still running).
    Carried(Document),
    /// The buffer has no analyzed facts yet.
    Missing,
}

impl RequestFacts {
    fn outcome(&self) -> &'static str {
        match self {
            RequestFacts::Current(_) => "current",
            RequestFacts::Carried(_) => "carried",
            RequestFacts::Missing => "missing",
        }
    }

    /// The facts a stale-safe request answers from: current or carried.
    pub fn stale_safe(self) -> Option<Document> {
        match self {
            RequestFacts::Current(current) => Some(current.document),
            RequestFacts::Carried(carried) => Some(carried),
            RequestFacts::Missing => None,
        }
    }

    /// The facts an edit-producing request answers from: current only. `Err` is the
    /// `ContentModified` error for an open document whose newest text has no facts yet;
    /// `Ok(None)` for a closed document the server has no facts for.
    pub fn edits(self, uri: &Uri) -> Result<Option<CurrentDocument>, tower_lsp_server::jsonrpc::Error> {
        match self {
            RequestFacts::Current(current) => Ok(Some(current)),
            RequestFacts::Carried(_) => Err(content_modified()),
            RequestFacts::Missing if open_version(uri).is_some() => Err(content_modified()),
            RequestFacts::Missing => Ok(None),
        }
    }
}

/// The LSP 3.17 `ContentModified` error (`-32801`): the document changed and the answer would
/// not match it; the client asks again.
pub fn content_modified() -> tower_lsp_server::jsonrpc::Error {
    tower_lsp_server::jsonrpc::Error::content_modified()
}

/// The facts of the newest text if they are stored within `wait`; otherwise the last analyzed
/// facts carried over to the newest text, or [`RequestFacts::Missing`].
///
/// Each stored analysis wakes the request. The stored facts are compared with the open text in
/// place; the document is copied once, when the request has its answer.
pub async fn document_facts(state: &RwLock<State>, uri: &Uri, wait: Duration) -> RequestFacts {
    let started = tokio::time::Instant::now();
    let mut lock_wait = Duration::ZERO;
    let facts = newest_facts(state, uri, wait, StateWait::Unbounded, &mut lock_wait)
        .await
        .expect("an unbounded state wait always gets the state");
    trace_facts(uri, wait, lock_wait, started, facts.outcome());
    facts
}

/// The facts of the newest text for an edit-producing request (rename, prepare rename, code
/// actions), when they are stored within `wait`. The budget covers every wait of the request:
/// the analysis, and also each read of the server state, which waits behind a queued writer.
/// Past the budget the answer is `ContentModified`; `Ok(None)` for a closed document the server
/// has no facts for.
pub async fn edit_document_facts(
    state: &RwLock<State>,
    uri: &Uri,
    wait: Duration,
) -> Result<Option<CurrentDocument>, tower_lsp_server::jsonrpc::Error> {
    let started = tokio::time::Instant::now();
    let mut lock_wait = Duration::ZERO;
    let facts = newest_facts(state, uri, wait, StateWait::WithinBudget, &mut lock_wait).await;
    trace_facts(uri, wait, lock_wait, started, facts.as_ref().map_or("state-busy", RequestFacts::outcome));
    match facts {
        Some(facts) => facts.edits(uri),
        None => Err(content_modified()),
    }
}

fn trace_facts(uri: &Uri, wait: Duration, lock_wait: Duration, started: tokio::time::Instant, outcome: &str) {
    crate::trace::event("facts", || {
        format!(
            "uri={} budget_ms={} lock_ms={} total_ms={} outcome={outcome}",
            uri.as_str(),
            wait.as_millis(),
            lock_wait.as_millis(),
            started.elapsed().as_millis(),
        )
    });
}

/// How a request waits for a read of the server state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StateWait {
    /// As long as a queued writer holds it (a read-only request then answers late, not wrong).
    Unbounded,
    /// Only within the request's budget: an edit-producing request fails closed instead.
    WithinBudget,
}

/// A read guard of the state, or `None` when `mode` is [`StateWait::WithinBudget`] and the
/// state is not free by `deadline`. The time spent waiting is added to `lock_wait`.
async fn read_state<'a>(
    state: &'a RwLock<State>,
    mode: StateWait,
    deadline: tokio::time::Instant,
    lock_wait: &mut Duration,
) -> Option<tokio::sync::RwLockReadGuard<'a, State>> {
    let asked = tokio::time::Instant::now();
    let guard = match mode {
        StateWait::Unbounded => Some(state.read().await),
        StateWait::WithinBudget => tokio::time::timeout_at(deadline, state.read()).await.ok(),
    };
    *lock_wait += asked.elapsed();
    guard
}

/// `None` only for [`StateWait::WithinBudget`], when a read of the state did not get it within
/// `wait`.
async fn newest_facts(
    state: &RwLock<State>,
    uri: &Uri,
    wait: Duration,
    mode: StateWait,
    lock_wait: &mut Duration,
) -> Option<RequestFacts> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let stored = Arc::clone(&read_state(state, mode, deadline, lock_wait).await?.facts_stored);
        let notified = stored.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        {
            let state = read_state(state, mode, deadline, lock_wait).await?;
            let document = stored_document(&state, uri);
            match (document, match_open_text(uri, document.map(|document| document.text.as_str()))) {
                (None, OpenMatch::Closed) => return Some(RequestFacts::Missing),
                (Some(document), OpenMatch::Closed) => {
                    return Some(RequestFacts::Current(CurrentDocument {
                        document: document.clone(),
                        open_version: None,
                    }));
                }
                (Some(document), OpenMatch::Current(version)) => {
                    let document = document.clone();
                    return Some(RequestFacts::Current(CurrentDocument { document, open_version: Some(version) }));
                }
                (_, OpenMatch::Current(_) | OpenMatch::Behind) => {}
            }
        }
        if tokio::time::timeout_at(deadline, notified).await.is_err() {
            return match mode {
                StateWait::Unbounded => {
                    let state = read_state(state, mode, deadline, lock_wait).await?;
                    Some(carried_from(&state, uri))
                }
                // Facts of an older text are no answer for an edit: the budget is spent, so the
                // request fails closed without carrying them over the edit (or waiting again).
                StateWait::WithinBudget => match open_version(uri) {
                    Some(_) => Some(RequestFacts::Missing),
                    None => {
                        let state = state.try_read().ok()?;
                        Some(match stored_document(&state, uri) {
                            Some(document) => RequestFacts::Current(CurrentDocument {
                                document: document.clone(),
                                open_version: None,
                            }),
                            None => RequestFacts::Missing,
                        })
                    }
                },
            };
        }
    }
}

/// The last analyzed facts, carried over to the newest text when its analysis is still
/// running. Never waits for analysis.
pub async fn latest_document(state: &RwLock<State>, uri: &Uri) -> Option<Document> {
    carried_facts(state, uri).await.stale_safe()
}

/// Wait (at most `limit`) until the facts of the newest text of `uri` are stored; `true` when
/// they are. Used only by background refresh tasks, never by a request.
pub async fn wait_for_current_facts(state: &RwLock<State>, uri: &Uri, limit: Duration) -> bool {
    matches!(document_facts(state, uri, limit).await, RequestFacts::Current(_))
}

async fn carried_facts(state: &RwLock<State>, uri: &Uri) -> RequestFacts {
    carried_from(&*state.read().await, uri)
}

fn carried_from(state: &State, uri: &Uri) -> RequestFacts {
    let Some(document) = stored_document(state, uri) else {
        return RequestFacts::Missing;
    };
    match match_open_text(uri, Some(document.text.as_str())) {
        OpenMatch::Closed => RequestFacts::Current(CurrentDocument { document: document.clone(), open_version: None }),
        OpenMatch::Current(version) => {
            RequestFacts::Current(CurrentDocument { document: document.clone(), open_version: Some(version) })
        }
        OpenMatch::Behind => match open_text(uri) {
            Some(newest) => RequestFacts::Carried(document.carried_over(newest, uri_to_path(uri).as_deref())),
            // Closed between the two reads: the stored facts are the disk text's.
            None => RequestFacts::Current(CurrentDocument { document: document.clone(), open_version: None }),
        },
    }
}

fn stored_document<'a>(state: &'a State, uri: &Uri) -> Option<&'a Document> {
    state.docs.get(uri).or_else(|| state.workspace_index.get(uri))
}
