//! Cancellation of an open document's analysis when the client sends a newer text.
//!
//! Each edit of an open document starts an analysis of its text. An analysis that is already
//! running when a newer text arrives is moot: its facts are never stored. It used to run to its
//! end anyway (seconds for a file of a large project), so the analysis of the newest text started
//! only after it and the facts lagged the last keystroke by up to two analyses. Now the newer text
//! cancels the running analysis: Salsa unwinds it at its next query and the project's database is
//! free for the newest text at once.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Mutex;

use beskid_queries::BeskidDatabase;
use salsa::{CancellationToken, Database};
use tower_lsp_server::ls_types::Uri;

/// The running analyses of open documents, one per document (the document's update gate
/// serializes its analyses).
#[derive(Default)]
pub(crate) struct RunningAnalyses {
    running: Mutex<HashMap<Uri, Running>>,
}

struct Running {
    id: u64,
    token: CancellationToken,
}

impl RunningAnalyses {
    /// Cancel the running analysis of `uri`, if any (the client sent a newer text or closed the
    /// document).
    pub(crate) fn cancel(&self, uri: &Uri) {
        if let Some(running) = self.lock().get(uri) {
            running.token.cancel();
        }
    }

    /// Run `work` on `db` as the analysis of `text` for `uri`. `None` when the client has sent a
    /// newer text (or closed the document) before or while it runs: the work is not started, or is
    /// unwound at its next database query.
    pub(crate) fn run<R>(
        &self,
        uri: &Uri,
        text: &str,
        db: &mut BeskidDatabase,
        work: impl FnOnce(&mut BeskidDatabase) -> R,
    ) -> Option<R> {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.lock().insert(uri.clone(), Running { id, token: db.cancellation_token() });
        // Registered first, then checked: a newer text that arrived before the registration is
        // seen here, and one that arrives after it cancels the work.
        let outcome = if super::lifecycle::superseded(uri, text) {
            None
        } else {
            salsa::Cancelled::catch(AssertUnwindSafe(|| work(&mut *db))).ok()
        };
        {
            let mut running = self.lock();
            if running.get(uri).is_some_and(|entry| entry.id == id) {
                running.remove(uri);
            }
        }
        // A cancellation that arrived after the work's last query is still pending on this
        // database handle; consume it here so it cannot unwind the next user of the database.
        let _ = salsa::Cancelled::catch(AssertUnwindSafe(|| db.attach(|db| db.unwind_if_revision_cancelled())));
        outcome
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uri, Running>> {
        self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use beskid_queries::BeskidDatabase;
    use tower_lsp_server::ls_types::Uri;

    use super::RunningAnalyses;
    use crate::features::formatting::open_texts;

    /// The client opens (or replaces) the text of `uri`, as the transport records it.
    fn open(uri: &Uri, text: &str) {
        let params = serde_json::json!({
            "textDocument": { "uri": uri.as_str(), "languageId": "beskid", "version": 1, "text": text }
        });
        open_texts::observe(&tower_lsp_server::jsonrpc::Request::build("textDocument/didOpen").params(params).finish());
    }

    /// What a database query does first: unwind when the analysis is cancelled.
    fn query(db: &BeskidDatabase) {
        use salsa::Database;
        db.attach(|db| db.unwind_if_revision_cancelled());
    }

    #[test]
    fn a_newer_text_unwinds_the_running_analysis_at_its_next_query() {
        let uri = Uri::from_str("file:///tmp/analysis_cancel/Running.bd").expect("URI");
        open(&uri, "first");
        let running = Arc::new(RunningAnalyses::default());
        let mut db = BeskidDatabase::default();
        let canceller = Arc::clone(&running);
        let cancel_uri = uri.clone();
        let outcome = running.run(&uri, "first", &mut db, |db| {
            // The client sends a newer text while the analysis runs.
            open(&cancel_uri, "second");
            canceller.cancel(&cancel_uri);
            query(db);
            "finished"
        });
        assert_eq!(outcome, None, "the cancelled analysis does not finish");
        // The database serves the next analysis normally.
        let next = running.run(&uri, "second", &mut db, |db| {
            query(db);
            "next"
        });
        assert_eq!(next, Some("next"));
    }

    #[test]
    fn a_text_superseded_before_the_start_is_not_analyzed() {
        let uri = Uri::from_str("file:///tmp/analysis_cancel/Superseded.bd").expect("URI");
        open(&uri, "newer");
        let running = RunningAnalyses::default();
        let mut db = BeskidDatabase::default();
        let mut ran = false;
        assert_eq!(running.run(&uri, "older", &mut db, |_| ran = true), None);
        assert!(!ran);
    }

    #[test]
    fn a_cancellation_after_the_last_query_does_not_reach_the_next_user() {
        let uri = Uri::from_str("file:///tmp/analysis_cancel/Late.bd").expect("URI");
        open(&uri, "text");
        let running = Arc::new(RunningAnalyses::default());
        let mut db = BeskidDatabase::default();
        let canceller = Arc::clone(&running);
        let cancel_uri = uri.clone();
        let outcome = running.run(&uri, "text", &mut db, |db| {
            query(db);
            // Cancelled after the last query (no query follows in this work).
            canceller.cancel(&cancel_uri);
            7
        });
        assert_eq!(outcome, Some(7), "the work finished before the cancellation could unwind it");
        // The pending cancellation was consumed: another user's query runs.
        let read = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| query(&db)));
        assert!(read.is_ok(), "a stale cancellation unwound the next user of the database");
        // After the analysis ended, a cancellation finds nothing to cancel.
        running.cancel(&uri);
        let read = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| query(&db)));
        assert!(read.is_ok());
    }
}
