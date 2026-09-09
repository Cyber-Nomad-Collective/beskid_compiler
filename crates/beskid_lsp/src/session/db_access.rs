//! Serialize access to the shared Salsa [`BeskidDatabase`] across concurrent LSP tasks.

use std::path::Path;
use std::sync::{Arc, Mutex};

use beskid_queries::{BeskidDatabase, configure_compilation_database_for_project, reset_compilation_database};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tower_lsp_server::ls_types::Uri;
use tracing::error;

use super::store::State;

fn recover_poisoned_database(
    poison: std::sync::PoisonError<std::sync::MutexGuard<'_, BeskidDatabase>>,
) -> BeskidDatabase {
    error!("compilation db lock poisoned; resetting Salsa database");
    let mut inner = poison.into_inner().clone();
    reset_compilation_database(&mut inner);
    inner
}

pub async fn with_compilation_db<R>(state: &RwLock<State>, f: impl FnOnce(&mut BeskidDatabase) -> R) -> R {
    with_compilation_db_mut_state(state, |db, _write| f(db)).await
}

/// Acquire a Salsa parallel-query handle.
///
/// Cloning a Salsa database clones only its thread-local handle; the memoized
/// storage and Beskid registries remain shared. The mutation gate is held only
/// while the handle is acquired. Salsa then permits multiple query handles to
/// execute concurrently and makes a later input write wait until they finish.
pub async fn parallel_compilation_db(state: &RwLock<State>) -> BeskidDatabase {
    let gate = state.read().await.db_gate.clone();
    let guard = gate.lock().await;
    let mut write = state.write().await;
    let db_arc = Arc::clone(&write.compilation_db);
    if db_arc.is_poisoned() {
        let inner = match db_arc.lock() {
            Err(poison) => recover_poisoned_database(poison),
            Ok(_) => unreachable!("compilation db reported poisoned"),
        };
        write.configured_project_root = None;
        write.compilation_db = Arc::new(Mutex::new(inner));
    }
    let snapshot = write.compilation_db.lock().expect("compilation db lock").clone();
    drop(write);
    drop(guard);
    snapshot
}

/// Mutate the shared Salsa writer configured for one project.
///
/// The LSP state write lock is held only for configuration bookkeeping. Salsa
/// writer exclusivity remains explicit, while unrelated document state stays
/// available during the mutation/query phase.
pub async fn with_compilation_db_for_project<R>(
    state: &RwLock<State>,
    project_root: &Path,
    f: impl FnOnce(&mut BeskidDatabase) -> R,
) -> R {
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
    let gate = state.read().await.db_gate.clone();
    let guard = gate.lock().await;
    let mut write = state.write().await;
    let db_arc = Arc::clone(&write.compilation_db);
    if db_arc.is_poisoned() {
        let inner = match db_arc.lock() {
            Err(poison) => recover_poisoned_database(poison),
            Ok(_) => unreachable!("compilation db reported poisoned"),
        };
        write.configured_project_root = None;
        write.compilation_db = Arc::new(Mutex::new(inner));
    }
    if write.configured_project_root.as_ref() != Some(&canonical) {
        let db_arc = Arc::clone(&write.compilation_db);
        configure_compilation_database_for_project(&mut db_arc.lock().expect("compilation db lock"), &canonical);
        write.configured_project_root = Some(canonical);
    }
    let db_arc = Arc::clone(&write.compilation_db);
    drop(write);
    let mut db = db_arc.lock().expect("compilation db lock");
    let result = f(&mut db);
    drop(db);
    drop(guard);
    result
}

pub async fn with_compilation_db_mut_state<R>(
    state: &RwLock<State>,
    f: impl FnOnce(&mut BeskidDatabase, &mut State) -> R,
) -> R {
    let gate = state.read().await.db_gate.clone();
    let _guard = gate.lock().await;
    let mut write = state.write().await;
    let db_arc = Arc::clone(&write.compilation_db);
    if db_arc.is_poisoned() {
        let inner = match db_arc.lock() {
            Err(poison) => recover_poisoned_database(poison),
            Ok(_) => unreachable!("compilation db reported poisoned"),
        };
        write.configured_project_root = None;
        write.compilation_db = Arc::new(Mutex::new(inner));
    }
    let db_arc = Arc::clone(&write.compilation_db);
    let mut db = db_arc.lock().expect("compilation db lock");
    f(&mut db, &mut write)
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

    use super::{document_update_gate, parallel_compilation_db};
    use crate::session::store::State;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn immutable_salsa_handles_can_query_concurrently() {
        let state = Arc::new(RwLock::new(State::default()));
        let rendezvous = Arc::new(Barrier::new(2));
        let first = parallel_compilation_db(&state).await;
        let second = parallel_compilation_db(&state).await;

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

    #[tokio::test]
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
