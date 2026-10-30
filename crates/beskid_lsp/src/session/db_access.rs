//! Access to the per-project Salsa databases from concurrent LSP tasks.

use std::path::Path;
use std::sync::Arc;

use beskid_queries::BeskidDatabase;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tower_lsp_server::ls_types::Uri;

use super::project_dbs::ProjectDatabase;
use super::store::State;

/// Run synchronous compiler work without stalling the async runtime: on the multi-threaded
/// server runtime the worker hands its other tasks (transport, other requests) to the rest of
/// the pool first.
pub(crate) fn run_blocking_section<R>(work: impl FnOnce() -> R) -> R {
    match tokio::runtime::Handle::try_current().map(|handle| handle.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(work),
        _ => work(),
    }
}

/// Which work an analysis serves; each kind has its own permits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lane {
    /// An open document (its edit, its diagnostics, a command).
    Interactive,
    /// The workspace index typing a project. It gets a fair turn against interactive work
    /// (see [`crate::scan_schedule::BackgroundTurn`]) and never queues behind closed files.
    Index,
    /// A closed file analyzed on its own (closing a document, a file changed on disk).
    ClosedFile,
}

impl Lane {
    fn from_interactive(interactive: bool) -> Self {
        if interactive { Self::Interactive } else { Self::ClosedFile }
    }
}

/// The workspace index's analysis of one project (see [`Lane::Index`]).
pub async fn with_index_db<R>(
    state: &RwLock<State>,
    project_root: &Path,
    f: impl FnOnce(BufferedSources, &mut BeskidDatabase) -> R,
) -> R {
    run_in_project_db_with(state, Some(project_root), Lane::Index, true, buffered_sources, f).await
}

/// Work in the database of `project_root` (the detached database for `None`) that keeps no facts
/// of its own (diagnostics assembly, explorer graphs): it shares the storage of the last
/// analysis. Only that project's gate is held, so work on another project does not wait; the
/// LSP state lock is held just long enough to look the database up. `interactive` marks work
/// for an open document, which keeps the project's database alive longer than scanned-only ones.
pub async fn with_shared_project_db<R>(
    state: &RwLock<State>,
    project_root: Option<&Path>,
    interactive: bool,
    f: impl FnOnce(&mut BeskidDatabase) -> R,
) -> R {
    run_in_project_db(state, project_root, Lane::from_interactive(interactive), false, f).await
}

/// Work in the database of `project_root` on `lane`. An `analysis` keeps its facts in documents
/// and starts from a fresh storage (see [`super::project_dbs::ANALYSES_PER_DATABASE`]);
/// otherwise the work shares the storage of the last analysis.
pub(crate) async fn run_in_project_db<R>(
    state: &RwLock<State>,
    project_root: Option<&Path>,
    lane: Lane,
    analysis: bool,
    f: impl FnOnce(&mut BeskidDatabase) -> R,
) -> R {
    run_in_project_db_with(state, project_root, lane, analysis, |_| (), |(), db| f(db)).await
}

/// The texts every project preparation feeds the database: the open documents and the
/// indexed files (about 10 MB on the Corelib).
pub(crate) type BufferedSources = Vec<(std::path::PathBuf, String)>;

/// The texts of the open documents and the indexed files, as the database must see them.
pub(crate) fn buffered_sources(state: &State) -> BufferedSources {
    state
        .workspace_index
        .iter()
        .chain(state.docs.iter())
        .filter_map(|(buffer_uri, document)| {
            crate::workspace_scan::uri_to_path(buffer_uri).map(|buffer_path| (buffer_path, document.text.clone()))
        })
        .collect()
}

/// [`run_in_project_db`] with an input taken from the state only once the work holds its permit
/// and the project's database. Work queued for a permit must hold nothing large: a preparation
/// that took its copy of every text before the wait held about 10 MB per queued edit or
/// publication, and under load the queue grew by a gigabyte a minute.
pub(crate) async fn run_in_project_db_with<P, R>(
    state: &RwLock<State>,
    project_root: Option<&Path>,
    lane: Lane,
    analysis: bool,
    prepare: impl FnOnce(&State) -> P,
    f: impl FnOnce(P, &mut BeskidDatabase) -> R,
) -> R {
    // The permit comes first: work waiting for its turn holds no database, so the databases
    // alive are bounded by the permits and the pool, however long the queue of edits grows.
    let (permits, turn) = {
        let read = state.read().await;
        let permits = match lane {
            Lane::Interactive => &read.analysis_permits,
            Lane::Index => &read.index_permits,
            Lane::ClosedFile => &read.background_permits,
        };
        (Arc::clone(permits), Arc::clone(&read.background_turn))
    };
    let waiting = std::time::Instant::now();
    let lane_name = match lane {
        Lane::Interactive => "interactive",
        Lane::Index => "index",
        Lane::ClosedFile => "closed",
    };
    let mut index_wait = match lane {
        Lane::Interactive => {
            turn.interactive_turn().await;
            None
        }
        Lane::Index => Some(turn.background_waiting()),
        Lane::ClosedFile => None,
    };
    let _permit = permits.acquire_owned().await.expect("analysis permits are never closed");
    let permit_ms = waiting.elapsed().as_millis();
    let (database, evicted) = state.write().await.databases.get(project_root, lane == Lane::Interactive);
    drop_in_background(evicted);
    let _gate = database.gate().lock().await;
    if let Some(wait) = index_wait.as_mut() {
        wait.started();
    }
    let gate_ms = waiting.elapsed().as_millis() - permit_ms;
    let _span = crate::trace::Span::start("db", || {
        format!(
            "lane={lane_name} analysis={analysis} permit_ms={permit_ms} gate_ms={gate_ms} project={}",
            project_root.map_or("-".to_string(), |root| root.display().to_string())
        )
    });
    let input = prepare(&*state.read().await);
    // Starting a fresh storage drops the previous one, which takes a while for a large project.
    run_blocking_section(|| {
        let mut db = if analysis { database.lock_for_analysis() } else { database.lock() };
        let result = f(input, &mut db);
        // The prepare spine caches each entry's compilation session process-wide (about 100 MB
        // for a Corelib package). The server keeps the facts it derived and starts every analysis
        // by invalidating the project's sessions, so a cached session is never read again; drop
        // it now instead of keeping one per project indexed.
        if let Some(root) = project_root {
            beskid_queries::invalidate_entry_sessions(root);
        }
        result
    })
}

/// Acquire a Salsa parallel-query handle on the database of `project_root`, if the pool holds
/// one (the facts of the project's documents come from it; a new database could answer nothing).
///
/// Cloning a Salsa database clones only its handle; the memoized storage and Beskid registries
/// remain shared. The project's gate is held only while the handle is acquired. Salsa then
/// lets query handles run concurrently, and a later input write cancels them and waits until
/// they are dropped.
pub async fn parallel_project_db(state: &RwLock<State>, project_root: Option<&Path>) -> Option<BeskidDatabase> {
    let database = state.read().await.databases.existing(project_root)?;
    let _gate = database.gate().lock().await;
    Some(database.lock().clone())
}

/// Drop databases on a blocking thread: freeing a large storage takes a while, and it must not
/// happen under the state lock or on an async worker.
pub(crate) fn drop_in_background(databases: Vec<ProjectDatabase>) {
    if databases.is_empty() {
        return;
    }
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => drop(handle.spawn_blocking(move || drop(databases))),
        Err(_) => drop(databases),
    }
}

pub(crate) fn new_db_gate() -> Arc<AsyncMutex<()>> {
    Arc::new(AsyncMutex::new(()))
}

/// Return the stable mutation fence for one document URI.
pub async fn document_update_gate(state: &RwLock<State>, uri: &Uri) -> Arc<AsyncMutex<()>> {
    let mut write = state.write().await;
    Arc::clone(write.document_update_gates.entry(uri.clone()).or_insert_with(new_db_gate))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use tokio::sync::RwLock;

    use super::{document_update_gate, parallel_project_db, with_shared_project_db};
    use crate::session::store::State;

    // Paused virtual time: a timeout expires only while the awaited future is truly blocked.
    #[tokio::test(start_paused = true)]
    async fn work_on_one_project_does_not_wait_for_another_project() {
        use std::path::Path;
        use std::time::Duration;

        let state = RwLock::new(State::default());
        let busy = state.write().await.databases.get(Some(Path::new("/w/busy")), true).0;
        let _held = busy.gate().lock().await;
        let other = tokio::time::timeout(
            Duration::from_millis(500),
            with_shared_project_db(&state, Some(Path::new("/w/other")), true, |_| 7),
        )
        .await;
        assert_eq!(other.ok(), Some(7), "another project's database is free");
        let same = tokio::time::timeout(
            Duration::from_millis(100),
            with_shared_project_db(&state, Some(Path::new("/w/busy")), true, |_| ()),
        )
        .await;
        assert!(same.is_err(), "the busy project's mutation gate is held");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn immutable_salsa_handles_can_query_concurrently() {
        let state = Arc::new(RwLock::new(State::default()));
        let rendezvous = Arc::new(Barrier::new(2));
        let first = parallel_project_db(&state, None).await.expect("detached database");
        let second = parallel_project_db(&state, None).await.expect("detached database");

        std::thread::scope(|scope| {
            let first_rendezvous = Arc::clone(&rendezvous);
            scope.spawn(move || {
                first_rendezvous.wait();
                assert!(!first.grammar_revision_ref().rev(&first).is_empty());
            });
            scope.spawn(move || {
                rendezvous.wait();
                assert!(!second.grammar_revision_ref().rev(&second).is_empty());
            });
        });
    }

    // Paused virtual time: a timeout expires only while the awaited future is truly blocked.
    #[tokio::test(start_paused = true)]
    async fn document_mutation_fences_do_not_block_unrelated_uris() {
        use std::str::FromStr;

        use tower_lsp_server::ls_types::Uri;

        let state = RwLock::new(State::default());
        let first_uri = Uri::from_str("file:///tmp/First.bd").expect("first URI");
        let second_uri = Uri::from_str("file:///tmp/Second.bd").expect("second URI");
        let first = document_update_gate(&state, &first_uri).await;
        let second = document_update_gate(&state, &second_uri).await;
        let _first_guard = first.lock().await;

        assert!(tokio::time::timeout(std::time::Duration::from_millis(50), second.lock()).await.is_ok());
    }
}
