//! Debounced/shutdown Salsa DB snapshot save hooks for the LSP session.
//!
//! The LSP loads the cross-run Salsa snapshot on project configuration (via
//! `configure_compilation_database_for_project` in `beskid_queries`), but it
//! never *saved* one — so edits made in the LSP never persisted for the next
//! session. This module wires the two save paths:
//!
//! 1. **Idle-debounce save**: each document change bumps a single global
//!    revision and schedules a save after the configured idle window (default
//!    5s). Rapid keystrokes coalesce into one save once the user stops typing.
//! 2. **Shutdown save**: `Backend::shutdown` calls [`save_snapshot_now`] so the
//!    session's work is flushed before the server exits.
//!
//! Both paths call [`beskid_queries::persist_session_snapshot`], which is a
//! no-op when the DB has no persistence root (unconfigured workspace) and logs
//! a warning — never panics — when the on-disk write fails. Snapshot persistence
//! is a performance optimization, not a correctness gate, so failures never
//! surface to the user or crash the LSP.
//!
//! The per-keystroke diagnostic path (`publish_diagnostics_for_uri`,
//! `build_syntax_facts`) deliberately does NOT save — that would defeat the
//! debounce. Only [`schedule_persistence_snapshot_save`] (idle) and
//! [`save_snapshot_now`] (shutdown) persist.

use std::sync::Arc;
use std::time::Duration;

use beskid_queries::persist_session_snapshot;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::session::{db_access::with_compilation_db, store::State};

/// Default idle debounce before the LSP persists the Salsa DB snapshot.
pub(crate) const DEFAULT_PERSISTENCE_DEBOUNCE: Duration = Duration::from_secs(5);

/// Parsed persistence settings from LSP initialization options or workspace config.
///
/// `None` fields mean "leave the existing value untouched" so partial updates
/// (e.g. only toggling `enabled` without resending the debounce) are honored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersistenceConfig {
    pub enabled: Option<bool>,
    pub debounce: Option<Duration>,
}

/// Parse persistence settings from LSP initialization options (top-level keys).
///
/// Mirrors the `logLevel` / `focusedProjectUri` top-level convention used by
/// [`crate::server::backend::Backend::initialize`]. Returns `None` when neither
/// key is present so callers can skip the state write lock entirely.
pub fn persistence_config_from_value(value: &Value) -> Option<PersistenceConfig> {
    let enabled = value.get("persistenceEnabled").and_then(Value::as_bool);
    let debounce_ms = value.get("persistenceSaveDebounceMs").and_then(Value::as_u64);
    if enabled.is_none() && debounce_ms.is_none() {
        return None;
    }
    Some(PersistenceConfig { enabled, debounce: debounce_ms.map(Duration::from_millis) })
}

/// Extract persistence settings from `didChangeConfiguration` settings JSON.
///
/// Reads `beskid.lsp.persistence.{enabled,saveDebounceMs}` to match the nested
/// `beskid.lsp.log.level` convention. Returns `None` when the
/// `beskid.lsp.persistence` node is absent.
pub fn persistence_config_from_configuration(settings: &Value) -> Option<PersistenceConfig> {
    let beskid = settings.get("beskid")?;
    let lsp = beskid.get("lsp")?;
    let persistence = lsp.get("persistence")?;
    let enabled = persistence.get("enabled").and_then(Value::as_bool);
    let debounce_ms = persistence.get("saveDebounceMs").and_then(Value::as_u64);
    if enabled.is_none() && debounce_ms.is_none() {
        return None;
    }
    Some(PersistenceConfig { enabled, debounce: debounce_ms.map(Duration::from_millis) })
}

/// Apply a parsed [`PersistenceConfig`] to the session state (partial update).
pub fn apply_persistence_config(state: &mut State, cfg: &PersistenceConfig) {
    if let Some(enabled) = cfg.enabled {
        state.persistence_save_enabled = enabled;
    }
    if let Some(debounce) = cfg.debounce {
        state.persistence_save_debounce = debounce;
    }
}

/// Persist the Salsa DB snapshot synchronously (shutdown path).
///
/// Bumps the save revision first so any pending debounced save observes a
/// mismatch and skips (avoids a redundant disk write after shutdown). Then
/// acquires the DB gate + write lock and calls `persist_session_snapshot`.
///
/// Failures are logged inside `persist_session_snapshot` and never propagate —
/// snapshot persistence is a performance optimization, not a correctness gate.
pub async fn save_snapshot_now(state: &RwLock<State>) {
    let enabled = {
        let mut write = state.write().await;
        // Cancel any pending debounced save so we don't write the same snapshot twice.
        write.persistence_save_revision = write.persistence_save_revision.saturating_add(1);
        write.persistence_save_enabled
    };
    if !enabled {
        return;
    }
    with_compilation_db(state, persist_session_snapshot).await;
}

/// Schedule a debounced Salsa DB snapshot save after the configured idle window.
///
/// Each call bumps a single global revision (the DB is shared across all URIs,
/// so one coalesced timer is correct) and spawns a task that sleeps for the
/// configured debounce. The task only persists when it is still the latest
/// scheduled save — rapid keystrokes coalesce into one save once the user
/// stops typing.
///
/// No-op (no spawn) when saves are disabled or the debounce window is zero.
pub async fn schedule_persistence_snapshot_save(state: Arc<RwLock<State>>) {
    let (revision, debounce, enabled) = {
        let mut write = state.write().await;
        write.persistence_save_revision = write.persistence_save_revision.saturating_add(1);
        (write.persistence_save_revision, write.persistence_save_debounce, write.persistence_save_enabled)
    };
    if !enabled || debounce.is_zero() {
        return;
    }
    let state_for_task = state;
    tokio::spawn(async move {
        tokio::time::sleep(debounce).await;
        let should_run = {
            let read = state_for_task.read().await;
            read.persistence_save_revision == revision
        };
        if should_run {
            save_snapshot_now(&state_for_task).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn persistence_config_from_value_reads_top_level_keys() {
        let cfg =
            persistence_config_from_value(&json!({"persistenceEnabled": false, "persistenceSaveDebounceMs": 1500}))
                .expect("present keys parse");
        assert_eq!(cfg.enabled, Some(false));
        assert_eq!(cfg.debounce, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn persistence_config_from_value_returns_none_when_absent() {
        assert!(persistence_config_from_value(&json!({"logLevel": "info"})).is_none());
    }

    #[test]
    fn persistence_config_from_value_partial_update_preserves_other_field() {
        let cfg = persistence_config_from_value(&json!({"persistenceEnabled": false})).expect("present key parses");
        assert_eq!(cfg.enabled, Some(false));
        assert!(cfg.debounce.is_none(), "absent debounce must stay None for partial update");
    }

    #[test]
    fn persistence_config_from_configuration_reads_nested_keys() {
        let settings = json!({"beskid": {"lsp": {"persistence": {"enabled": true, "saveDebounceMs": 7000}}}});
        let cfg = persistence_config_from_configuration(&settings).expect("nested keys parse");
        assert_eq!(cfg.enabled, Some(true));
        assert_eq!(cfg.debounce, Some(Duration::from_secs(7)));
    }

    #[test]
    fn persistence_config_from_configuration_returns_none_when_node_absent() {
        assert!(
            persistence_config_from_configuration(&json!({"beskid": {"lsp": {"log": {"level": "info"}}}})).is_none()
        );
    }

    #[test]
    fn apply_persistence_config_partial_update_leaves_unset_fields() {
        let mut state = State::default();
        let original_debounce = state.persistence_save_debounce;
        apply_persistence_config(&mut state, &PersistenceConfig { enabled: Some(false), debounce: None });
        assert!(!state.persistence_save_enabled);
        assert_eq!(state.persistence_save_debounce, original_debounce, "unset debounce must be preserved");
    }

    #[tokio::test]
    async fn save_snapshot_now_skips_when_disabled() {
        let state = RwLock::new(State { persistence_save_enabled: false, ..State::default() });
        // No persistence root configured either way, but disabled must short-circuit
        // before even reaching the DB access path.
        save_snapshot_now(&state).await;
        let read = state.read().await;
        assert!(!read.persistence_save_enabled);
    }

    #[tokio::test]
    async fn save_snapshot_now_bumps_revision_to_cancel_pending_debounce() {
        let state = RwLock::new(State::default());
        let before = state.read().await.persistence_save_revision;
        save_snapshot_now(&state).await;
        let after = state.read().await.persistence_save_revision;
        assert_eq!(after, before.saturating_add(1), "shutdown save must cancel pending debounced saves");
    }
}
