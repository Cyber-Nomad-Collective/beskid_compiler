//! Registry operations: blocked-link policy, activity log and the weekly
//! in-app spotlight. Roles are projected from Authelia groups by the HTTP
//! adapter, so this module never reads a persisted role table.

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use beskid_pckg_auth::{Principal, SubjectRole};
use beskid_pckg_operations::BlockedLinkPatterns;
use beskid_pckg_store::{
    AsyncRegistryOperationsRepository, BlockedLinkPolicy, NewBlockedLinkPolicy, NewRegistryActivity, RegistryActivity,
    RegistryOperationsStoreError, SqlxPackageRepository, WeeklySpotlightRun,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, authenticated_principal, unauthorized_response};

#[derive(Clone)]
pub(crate) struct OperationsState {
    backend: OperationsBackend,
}

#[derive(Clone)]
enum OperationsBackend {
    InMemory(Arc<Mutex<InMemoryOperations>>),
    Sqlx(Arc<SqlxPackageRepository>),
}

#[derive(Default)]
struct InMemoryOperations {
    blocked_links: Vec<BlockedLinkPolicy>,
    activity: Vec<RegistryActivity>,
    next_sequence: i64,
    spotlights: Vec<WeeklySpotlightRun>,
}

impl OperationsState {
    pub(crate) fn in_memory() -> Self {
        Self { backend: OperationsBackend::InMemory(Arc::new(Mutex::new(InMemoryOperations::default()))) }
    }

    pub(crate) fn sqlx(repository: Arc<SqlxPackageRepository>) -> Self {
        Self { backend: OperationsBackend::Sqlx(repository) }
    }

    /// A principal is a super admin when Authelia projected the configured admin
    /// group (mock mode mints the admin role directly). The registry never
    /// consults a persisted role table.
    pub(crate) fn is_super_admin(&self, principal: &Principal) -> bool {
        principal.has_role(SubjectRole::SuperAdmin)
    }

    pub(crate) async fn append_activity(
        &self,
        activity: NewRegistryActivity,
    ) -> Result<RegistryActivity, RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.append_registry_activity(activity).await,
            OperationsBackend::InMemory(operations) => {
                validate_in_memory_activity(&activity)?;
                let mut operations = operations.lock().expect("operations mutex is not poisoned");
                operations.next_sequence += 1;
                let entry = RegistryActivity {
                    sequence: operations.next_sequence,
                    occurred_at_unix_seconds: activity.occurred_at_unix_seconds,
                    severity: activity.severity,
                    action: activity.action,
                    message: activity.message,
                    trace_id: activity.trace_id,
                    actor_subject: activity.actor_subject,
                    package_name: activity.package_name,
                    version: activity.version,
                };
                operations.activity.push(entry.clone());
                operations.activity.sort_by(|left, right| {
                    right
                        .occurred_at_unix_seconds
                        .cmp(&left.occurred_at_unix_seconds)
                        .then_with(|| right.sequence.cmp(&left.sequence))
                });
                operations.activity.truncate(500);
                Ok(entry)
            }
        }
    }

    async fn list_blocked_links(&self) -> Result<Vec<BlockedLinkPolicy>, RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.list_blocked_link_policies().await,
            OperationsBackend::InMemory(operations) => {
                Ok(operations.lock().expect("operations mutex is not poisoned").blocked_links.clone())
            }
        }
    }

    /// Reads the durable blocked-link policy through the one shared domain
    /// matcher. Package-review routes call this at their mutation boundary;
    /// they neither cache nor reinterpret policy rows.
    pub(crate) async fn block_reason(&self, text: &str) -> Result<Option<&'static str>, RegistryOperationsStoreError> {
        let policies = self.list_blocked_links().await?;
        let patterns = BlockedLinkPatterns::from_patterns(policies.iter().map(|policy| policy.pattern.as_str()))
            .map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkPattern)?;
        Ok(patterns.block_reason(text))
    }

    async fn add_blocked_link(
        &self,
        policy: NewBlockedLinkPolicy,
    ) -> Result<BlockedLinkPolicy, RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.add_blocked_link_policy(policy).await,
            OperationsBackend::InMemory(operations) => {
                let pattern = policy.pattern.trim();
                if pattern.is_empty() || pattern.len() > 512 {
                    return Err(RegistryOperationsStoreError::InvalidBlockedLinkPattern);
                }
                if !is_valid_subject(&policy.created_by_subject) {
                    return Err(RegistryOperationsStoreError::InvalidAuthHubSubject);
                }
                let mut operations = operations.lock().expect("operations mutex is not poisoned");
                if operations.blocked_links.iter().any(|existing| existing.pattern.eq_ignore_ascii_case(pattern)) {
                    return Err(RegistryOperationsStoreError::DuplicateBlockedLinkPattern);
                }
                let policy = BlockedLinkPolicy {
                    id: policy.id,
                    pattern: pattern.to_owned(),
                    note: normalize_note(policy.note)?,
                    created_by_subject: policy.created_by_subject,
                    created_at_unix_seconds: policy.created_at_unix_seconds,
                };
                operations.blocked_links.push(policy.clone());
                operations.blocked_links.sort_by_key(|policy| std::cmp::Reverse(policy.created_at_unix_seconds));
                Ok(policy)
            }
        }
    }

    async fn delete_blocked_link(&self, id: &str) -> Result<(), RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.delete_blocked_link_policy(id).await,
            OperationsBackend::InMemory(operations) => {
                let mut operations = operations.lock().expect("operations mutex is not poisoned");
                let before = operations.blocked_links.len();
                operations.blocked_links.retain(|policy| policy.id != id);
                (before != operations.blocked_links.len()).then_some(()).ok_or(RegistryOperationsStoreError::NotFound)
            }
        }
    }

    async fn recent_activity(&self, take: u16) -> Result<Vec<RegistryActivity>, RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.recent_registry_activity(take).await,
            OperationsBackend::InMemory(operations) => Ok(operations
                .lock()
                .expect("operations mutex is not poisoned")
                .activity
                .iter()
                .take(usize::from(take.clamp(1, 500)))
                .cloned()
                .collect()),
        }
    }

    async fn record_spotlight(
        &self,
        run: WeeklySpotlightRun,
    ) -> Result<WeeklySpotlightRun, RegistryOperationsStoreError> {
        match &self.backend {
            OperationsBackend::Sqlx(repository) => repository.record_weekly_spotlight(run).await,
            OperationsBackend::InMemory(operations) => {
                let mut operations = operations.lock().expect("operations mutex is not poisoned");
                operations.spotlights.push(run.clone());
                Ok(run)
            }
        }
    }
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/blocked-links", get(list_blocked_links).post(add_blocked_link))
        .route("/api/admin/blocked-links/{id}", delete(delete_blocked_link))
        .route("/api/admin/registry-activity", get(registry_activity))
        .route("/api/admin/notifications/weekly-spotlight/run", post(run_weekly_spotlight))
}

#[derive(Deserialize)]
struct AddBlockedLinkRequest {
    pattern: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct ActivityQuery {
    take: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockedLinkResponse {
    id: String,
    pattern: String,
    note: Option<String>,
    created_at_utc: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddBlockedLinkResponse {
    success: bool,
    message: &'static str,
    item: BlockedLinkResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityResponse {
    timestamp_utc: String,
    severity: String,
    action: String,
    message: String,
    trace_id: Option<String>,
    user_id: Option<String>,
    package_name: Option<String>,
    version: Option<String>,
}

async fn list_blocked_links(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    if !state.operations.is_super_admin(&principal) {
        return forbidden();
    }
    match state.operations.list_blocked_links().await {
        Ok(policies) => Json(policies.into_iter().map(blocked_link_response).collect::<Vec<_>>()).into_response(),
        Err(_) => unavailable(),
    }
}

async fn add_blocked_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddBlockedLinkRequest>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    if !state.operations.is_super_admin(&principal) {
        return forbidden();
    }
    let outcome = state
        .operations
        .add_blocked_link(NewBlockedLinkPolicy {
            id: Uuid::new_v4().to_string(),
            pattern: request.pattern,
            note: request.note,
            created_by_subject: principal.subject().to_owned(),
            created_at_unix_seconds: crate::now_unix_seconds(),
        })
        .await;
    match outcome {
        Ok(policy) => Json(AddBlockedLinkResponse {
            success: true,
            message: "Pattern added.",
            item: blocked_link_response(policy),
        })
        .into_response(),
        Err(RegistryOperationsStoreError::DuplicateBlockedLinkPattern) => conflict("that pattern is already blocked"),
        Err(RegistryOperationsStoreError::InvalidBlockedLinkPattern) => bad_request("invalid blocked-link pattern"),
        Err(_) => unavailable(),
    }
}

async fn delete_blocked_link(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    if !state.operations.is_super_admin(&principal) {
        return forbidden();
    }
    match state.operations.delete_blocked_link(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(RegistryOperationsStoreError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(RegistryOperationsStoreError::InvalidBlockedLinkId) => bad_request("invalid blocked-link id"),
        Err(_) => unavailable(),
    }
}

async fn registry_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActivityQuery>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    if !state.operations.is_super_admin(&principal) {
        return forbidden();
    }
    let take = query.take.filter(|take| *take > 0).unwrap_or(200).min(500);
    match state.operations.recent_activity(take).await {
        Ok(activity) => Json(activity.into_iter().map(activity_response).collect::<Vec<_>>()).into_response(),
        Err(_) => unavailable(),
    }
}

async fn run_weekly_spotlight(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    if !state.operations.is_super_admin(&principal) {
        return forbidden();
    }
    let now = crate::now_unix_seconds();
    let activity_count = match state.operations.recent_activity(500).await {
        Ok(entries) => {
            entries.iter().filter(|entry| entry.occurred_at_unix_seconds >= now - 7 * 24 * 60 * 60).count() as u64
        }
        Err(_) => return unavailable(),
    };
    let run = WeeklySpotlightRun {
        id: Uuid::new_v4().to_string(),
        ran_by_subject: principal.subject().to_owned(),
        ran_at_unix_seconds: now,
        activity_count,
        delivery: "in_app_only".to_owned(),
    };
    if state.operations.record_spotlight(run).await.is_err()
        || state
            .operations
            .append_activity(NewRegistryActivity {
                occurred_at_unix_seconds: now,
                severity: "Information".to_owned(),
                action: "weekly_spotlight_run".to_owned(),
                message: "Weekly spotlight evaluated for in-app delivery; SMTP is retired.".to_owned(),
                trace_id: None,
                actor_subject: Some(principal.subject().to_owned()),
                package_name: None,
                version: None,
            })
            .await
            .is_err()
    {
        return unavailable();
    }
    Json(serde_json::json!({"ok": true, "activityCount": activity_count, "delivery": "in_app_only"})).into_response()
}

fn blocked_link_response(policy: BlockedLinkPolicy) -> BlockedLinkResponse {
    BlockedLinkResponse {
        id: policy.id,
        pattern: policy.pattern,
        note: policy.note,
        created_at_utc: crate::format_timestamp(policy.created_at_unix_seconds),
    }
}

fn activity_response(activity: RegistryActivity) -> ActivityResponse {
    ActivityResponse {
        timestamp_utc: crate::format_timestamp(activity.occurred_at_unix_seconds),
        severity: activity.severity,
        action: activity.action,
        message: activity.message,
        trace_id: activity.trace_id,
        user_id: activity.actor_subject,
        package_name: activity.package_name,
        version: activity.version,
    }
}

fn normalize_note(note: Option<String>) -> Result<Option<String>, RegistryOperationsStoreError> {
    let note = note.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()));
    if note.as_ref().is_some_and(|value| value.len() > 2000) {
        return Err(RegistryOperationsStoreError::InvalidBlockedLinkPattern);
    }
    Ok(note)
}

fn validate_in_memory_activity(activity: &NewRegistryActivity) -> Result<(), RegistryOperationsStoreError> {
    if activity.severity.trim().is_empty() || activity.action.trim().is_empty() || activity.message.len() > 4000 {
        return Err(RegistryOperationsStoreError::InvalidActivity);
    }
    if activity.actor_subject.as_deref().is_some_and(|subject| !is_valid_subject(subject)) {
        return Err(RegistryOperationsStoreError::InvalidAuthHubSubject);
    }
    Ok(())
}

fn is_valid_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    !trimmed.is_empty()
        && trimmed == subject
        && trimmed.bytes().all(|byte| byte.is_ascii_graphic())
        && trimmed.len() <= 255
}

fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": message}))).into_response()
}

fn conflict(message: &'static str) -> Response {
    (StatusCode::CONFLICT, Json(serde_json::json!({"message": message}))).into_response()
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({"message": "forbidden"}))).into_response()
}

fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"message": "registry operations unavailable"})))
        .into_response()
}
