//! Startup sequencing: defer Salsa-backed work until the first workspace scan completes.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio::sync::RwLock;

use super::store::State;

/// Block until the first [`crate::workspace_scan::scan_workspace`] pass has finished.
pub async fn wait_for_initial_scan(state: &RwLock<State>) {
    loop {
        let notify = {
            let read = state.read().await;
            if read.initial_scan_complete.load(Ordering::Acquire) {
                return;
            }
            Arc::clone(&read.scan_barrier)
        };
        notify.notified().await;
    }
}

/// Mark the initial workspace scan complete and wake waiters.
pub async fn signal_initial_scan_complete(state: &RwLock<State>) {
    state.read().await.mark_initial_scan_complete();
}
