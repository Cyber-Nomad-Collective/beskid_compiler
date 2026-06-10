//! Serialize access to the shared Salsa [`BeskidDatabase`] across concurrent LSP tasks.

use std::sync::Arc;

use beskid_queries::BeskidDatabase;
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use super::store::State;

pub async fn with_compilation_db<R>(
    state: &RwLock<State>,
    f: impl FnOnce(&mut BeskidDatabase) -> R,
) -> R {
    let gate = state.read().await.db_gate.clone();
    let _guard = gate.lock().await;
    let write = state.write().await;
    let db_arc = Arc::clone(&write.compilation_db);
    drop(write);
    let mut db = db_arc.lock().expect("compilation db lock");
    f(&mut db)
}

pub async fn with_compilation_db_mut_state<R>(
    state: &RwLock<State>,
    f: impl FnOnce(&mut BeskidDatabase, &mut State) -> R,
) -> R {
    let gate = state.read().await.db_gate.clone();
    let _guard = gate.lock().await;
    let mut write = state.write().await;
    let db_arc = Arc::clone(&write.compilation_db);
    let mut db = db_arc.lock().expect("compilation db lock");
    f(&mut db, &mut write)
}

pub(crate) fn new_db_gate() -> Arc<AsyncMutex<()>> {
    Arc::new(AsyncMutex::new(()))
}
