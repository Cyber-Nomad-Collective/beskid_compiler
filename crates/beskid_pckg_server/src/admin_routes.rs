//! Persisted registry administration. All requests begin with an Authelia
//! session; roles are projected from Authelia groups, so the registry only
//! persists publisher verification, per-resource moderation grants and the
//! package-review audit log.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_auth::{Principal, SubjectRole};
use beskid_pckg_store::{
    AsyncAdministrationRepository, PackageReviewDecision, PublisherVerification, ResourcePermissionGrant,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{AppState, authenticated_principal, unauthorized_response};

#[derive(Deserialize)]
pub(crate) struct VerificationRequest {
    verified: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrantPermissionRequest {
    subject: String,
    resource: String,
    capability: String,
}
#[derive(Deserialize)]
pub(crate) struct ReviewRequest {
    decision: String,
    #[serde(default)]
    reason: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateUserRequest {
    #[serde(default)]
    roles: Vec<String>,
    #[serde(rename = "publisherVerified")]
    publisher_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserResponse {
    subject: String,
    display_name: String,
    publisher_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionResponse {
    subject: String,
    resource: String,
    capability: String,
}

pub(crate) async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys.clone() else {
        return unavailable();
    };
    if !is_super_admin(&principal) {
        return forbidden();
    }
    let verifications = match repository.list_publisher_verifications().await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    let permissions = match repository.list_all_resource_permissions().await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    let mut users: BTreeMap<String, bool> = BTreeMap::new();
    for verification in verifications {
        users.insert(verification.subject, verification.is_verified);
    }
    for grant in permissions {
        users.entry(grant.subject).or_default();
    }
    Json(
        users
            .into_iter()
            .map(|(subject, publisher_verified)| AdminUserResponse {
                display_name: subject.clone(),
                subject,
                publisher_verified,
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(crate) async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys.clone() else {
        return unavailable();
    };
    if !is_super_admin(&principal) {
        return forbidden();
    }
    // Roles are Authelia-managed; the legacy `roles` field is accepted but
    // intentionally ignored so the frontend can stop sending it without a
    // coordinated break.
    let _ = request.roles;
    if repository
        .set_publisher_verification(PublisherVerification {
            subject: target.clone(),
            is_verified: request.publisher_verified,
            reviewed_by_subject: principal.subject().to_owned(),
            reviewed_at_unix_seconds: now(),
        })
        .await
        .is_err()
    {
        return bad_request("invalid administration update");
    }
    Json(AdminUserResponse {
        display_name: target.clone(),
        subject: target,
        publisher_verified: request.publisher_verified,
    })
    .into_response()
}

pub(crate) async fn set_publisher_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target): Path<String>,
    Json(request): Json<VerificationRequest>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys.clone() else {
        return unavailable();
    };
    if !is_super_admin(&principal) {
        return forbidden();
    }
    match repository
        .set_publisher_verification(PublisherVerification {
            subject: target.clone(),
            is_verified: request.verified,
            reviewed_by_subject: principal.subject().to_owned(),
            reviewed_at_unix_seconds: now(),
        })
        .await
    {
        Ok(()) => Json(serde_json::json!({"subject": target, "isVerified": request.verified})).into_response(),
        Err(_) => bad_request("invalid publisher subject"),
    }
}

pub(crate) async fn list_permissions(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&principal) {
        return forbidden();
    }
    match repository.list_all_resource_permissions().await {
        Ok(grants) => Json(grants.into_iter().map(permission_response).collect::<Vec<_>>()).into_response(),
        Err(_) => bad_request("invalid resource"),
    }
}

pub(crate) async fn grant_permission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GrantPermissionRequest>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&principal) {
        return forbidden();
    }
    let Some((resource_kind, resource_id)) = request.resource.split_once(':') else {
        return bad_request("resource must be kind:id");
    };
    if request.capability != "moderate" {
        return bad_request("capability must be moderate");
    }
    let response = PermissionResponse {
        subject: request.subject.clone(),
        resource: request.resource.clone(),
        capability: request.capability.clone(),
    };
    match repository
        .grant_resource_permission(ResourcePermissionGrant {
            subject: request.subject,
            resource_kind: resource_kind.to_owned(),
            resource_id: resource_id.to_owned(),
            capability: request.capability,
            granted_by_subject: principal.subject().to_owned(),
            granted_at_unix_seconds: now(),
        })
        .await
    {
        Ok(()) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(_) => bad_request("invalid permission grant"),
    }
}

pub(crate) async fn review_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((name, version)): Path<(String, String)>,
    Json(request): Json<ReviewRequest>,
) -> Response {
    let Some(principal) = authenticated_principal(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys.clone() else {
        return unavailable();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(Some(package)) => package,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    if !can_moderate(&principal, &*repository, &package.owner_subject, &package.id).await {
        return not_found();
    }
    if !matches!(request.decision.as_str(), "approved" | "rejected" | "yanked" | "unyanked") {
        return bad_request("invalid review decision");
    }
    if matches!(request.decision.as_str(), "yanked" | "unyanked") {
        let yanked = request.decision == "yanked";
        if state.packages.set_yanked(&package.id, &version, yanked, now()).await.is_err() {
            return not_found();
        }
    }
    match repository
        .record_package_review(PackageReviewDecision {
            package_id: package.id,
            version: Some(version),
            decision: request.decision,
            reason: request.reason,
            decided_by_subject: principal.subject().to_owned(),
            decided_at_unix_seconds: now(),
        })
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => bad_request("invalid review decision"),
    }
}

fn is_super_admin(principal: &Principal) -> bool {
    principal.has_role(SubjectRole::SuperAdmin)
}

async fn can_moderate(
    principal: &Principal,
    repository: &dyn AsyncAdministrationRepository,
    owner: &str,
    package_id: &str,
) -> bool {
    if principal.subject() == owner {
        return true;
    }
    if principal.has_role(SubjectRole::SuperAdmin) || principal.has_role(SubjectRole::Moderator) {
        return true;
    }
    repository
        .list_resource_permissions("package", package_id)
        .await
        .map(|grants| grants.iter().any(|grant| grant.subject == principal.subject() && grant.capability == "moderate"))
        .unwrap_or(false)
}

fn permission_response(value: ResourcePermissionGrant) -> PermissionResponse {
    PermissionResponse {
        subject: value.subject,
        resource: format!("{}:{}", value.resource_kind, value.resource_id),
        capability: value.capability,
    }
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs() as i64
}
fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({"message":"administrator access required"}))).into_response()
}
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({"message":"package not found"}))).into_response()
}
fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":message}))).into_response()
}
fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"message":"administration persistence is not configured"})),
    )
        .into_response()
}
