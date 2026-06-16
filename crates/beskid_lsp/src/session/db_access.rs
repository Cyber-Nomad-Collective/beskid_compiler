//! Serialize access to the shared Salsa [`BeskidDatabase`] across concurrent LSP tasks.

use std::sync::{Arc, Mutex};

use beskid_queries::{BeskidDatabase, reset_compilation_database};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
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

pub async fn with_compilation_db<R>(
    state: &RwLock<State>,
    f: impl FnOnce(&mut BeskidDatabase) -> R,
) -> R {
    with_compilation_db_mut_state(state, |db, _write| f(db)).await
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
