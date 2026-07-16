//! Persisted registry administration. All requests begin with an Auth Hub
//! session; PostgreSQL is the only authority for elevated roles and grants.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_store::{
    AdminRole, AdminRoleAssignment, AsyncAdministrationRepository, PackageReviewDecision,
    PublisherVerification, ResourcePermissionGrant,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{AppState, authenticated_subject, unauthorized_response};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoleResponse {
    subject: String,
    role: String,
    granted_by_subject: String,
    granted_at_utc: String,
}
#[derive(Deserialize)]
pub(crate) struct GrantRoleRequest {
    role: String,
}
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionResponse {
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
pub(crate) struct UpdateUserRequest {
    roles: Vec<String>,
    #[serde(rename = "publisherVerified")]
    publisher_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserResponse {
    subject: String,
    github_login: String,
    roles: Vec<String>,
    publisher_verified: bool,
}

pub(crate) async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
        return forbidden();
    }
    let roles = match repository.list_admin_roles().await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    let verifications = match repository.list_publisher_verifications().await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    let mut users = BTreeMap::<String, (BTreeSet<String>, bool)>::new();
    for role in roles {
        let subject = role.subject.clone();
        users
            .entry(subject)
            .or_default()
            .0
            .insert(role_response(role).role);
    }
    for verification in verifications {
        users.entry(verification.subject).or_default().1 = verification.is_verified;
    }
    Json(
        users
            .into_iter()
            .map(|(subject, (roles, publisher_verified))| AdminUserResponse {
                github_login: subject.clone(),
                subject,
                roles: std::iter::once("Member".to_owned())
                    .chain(roles.into_iter().map(|role| {
                        if role == "moderator" {
                            "Moderator".to_owned()
                        } else {
                            "SuperAdmin".to_owned()
                        }
                    }))
                    .collect(),
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
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
        return forbidden();
    }
    let mut roles = Vec::new();
    for role in &request.roles {
        match role.as_str() {
            "Member" => {}
            "Moderator" => roles.push(AdminRole::Moderator),
            "SuperAdmin" => roles.push(AdminRole::SuperAdmin),
            _ => return bad_request("invalid role"),
        }
    }
    if repository
        .replace_admin_roles(&target, roles.clone(), &actor, now())
        .await
        .is_err()
        || repository
            .set_publisher_verification(PublisherVerification {
                subject: target.clone(),
                is_verified: request.publisher_verified,
                reviewed_by_subject: actor,
                reviewed_at_unix_seconds: now(),
            })
            .await
            .is_err()
    {
        return bad_request("invalid administration update");
    }
    Json(AdminUserResponse {
        github_login: target.clone(),
        subject: target,
        roles: std::iter::once("Member".to_owned())
            .chain(roles.into_iter().map(|role| match role {
                AdminRole::Moderator => "Moderator".to_owned(),
                AdminRole::SuperAdmin => "SuperAdmin".to_owned(),
                AdminRole::User => "Member".to_owned(),
            }))
            .collect(),
        publisher_verified: request.publisher_verified,
    })
    .into_response()
}

pub(crate) async fn list_roles(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &subject).await {
        return forbidden();
    }
    match repository.list_admin_roles().await {
        Ok(roles) => Json(roles.into_iter().map(role_response).collect::<Vec<_>>()).into_response(),
        Err(_) => unavailable(),
    }
}

pub(crate) async fn grant_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target): Path<String>,
    Json(request): Json<GrantRoleRequest>,
) -> Response {
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
        return forbidden();
    }
    let role = match request.role.as_str() {
        "moderator" => AdminRole::Moderator,
        "superadmin" => AdminRole::SuperAdmin,
        _ => return bad_request("role must be moderator or superadmin"),
    };
    let assignment = AdminRoleAssignment {
        subject: target,
        role,
        granted_by_subject: actor,
        granted_at_unix_seconds: now(),
    };
    match repository.grant_admin_role(assignment).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => bad_request("invalid role grant"),
    }
}

pub(crate) async fn set_publisher_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target): Path<String>,
    Json(request): Json<VerificationRequest>,
) -> Response {
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
        return forbidden();
    }
    match repository
        .set_publisher_verification(PublisherVerification {
            subject: target.clone(),
            is_verified: request.verified,
            reviewed_by_subject: actor,
            reviewed_at_unix_seconds: now(),
        })
        .await
    {
        Ok(()) => Json(serde_json::json!({"subject": target, "isVerified": request.verified}))
            .into_response(),
        Err(_) => bad_request("invalid publisher subject"),
    }
}

pub(crate) async fn list_permissions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
        return forbidden();
    }
    match repository.list_all_resource_permissions().await {
        Ok(grants) => Json(
            grants
                .into_iter()
                .map(permission_response)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => bad_request("invalid resource"),
    }
}

pub(crate) async fn grant_permission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GrantPermissionRequest>,
) -> Response {
    let Some(actor) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    if !is_super_admin(&*repository, &actor).await {
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
            granted_by_subject: actor,
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
    let Some(actor) = authenticated_subject(&state, &headers) else {
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
    if !can_moderate(&*repository, &actor, &package.owner_subject, &package.id).await {
        return not_found();
    }
    if !matches!(
        request.decision.as_str(),
        "approved" | "rejected" | "yanked" | "unyanked"
    ) {
        return bad_request("invalid review decision");
    }
    if matches!(request.decision.as_str(), "yanked" | "unyanked") {
        let yanked = request.decision == "yanked";
        if state
            .packages
            .set_yanked(&package.id, &version, yanked, now())
            .await
            .is_err()
        {
            return not_found();
        }
    }
    match repository
        .record_package_review(PackageReviewDecision {
            package_id: package.id,
            version: Some(version),
            decision: request.decision,
            reason: request.reason,
            decided_by_subject: actor,
            decided_at_unix_seconds: now(),
        })
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => bad_request("invalid review decision"),
    }
}

async fn is_super_admin(repository: &dyn AsyncAdministrationRepository, subject: &str) -> bool {
    repository
        .roles_for_subject(subject)
        .await
        .map(|roles| roles.contains(&AdminRole::SuperAdmin))
        .unwrap_or(false)
}
async fn can_moderate(
    repository: &dyn AsyncAdministrationRepository,
    subject: &str,
    owner: &str,
    package_id: &str,
) -> bool {
    if subject == owner {
        return true;
    }
    if repository
        .roles_for_subject(subject)
        .await
        .map(|roles| {
            roles
                .iter()
                .any(|role| matches!(role, AdminRole::Moderator | AdminRole::SuperAdmin))
        })
        .unwrap_or(false)
    {
        return true;
    }
    repository
        .list_resource_permissions("package", package_id)
        .await
        .map(|grants| {
            grants
                .iter()
                .any(|grant| grant.subject == subject && grant.capability == "moderate")
        })
        .unwrap_or(false)
}
fn role_response(value: AdminRoleAssignment) -> RoleResponse {
    RoleResponse {
        subject: value.subject,
        role: match value.role {
            AdminRole::User => "user",
            AdminRole::Moderator => "moderator",
            AdminRole::SuperAdmin => "superadmin",
        }
        .to_owned(),
        granted_by_subject: value.granted_by_subject,
        granted_at_utc: rfc3339(value.granted_at_unix_seconds),
    }
}
fn permission_response(value: ResourcePermissionGrant) -> PermissionResponse {
    PermissionResponse {
        subject: value.subject,
        resource: format!("{}:{}", value.resource_kind, value.resource_id),
        capability: value.capability,
    }
}
fn rfc3339(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .expect("timestamps are valid")
        .to_rfc3339()
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs() as i64
}
fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"message":"administrator access required"})),
    )
        .into_response()
}
fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"message":"package not found"})),
    )
        .into_response()
}
fn bad_request(message: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"message":message})),
    )
        .into_response()
}
fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"message":"administration persistence is not configured"})),
    )
        .into_response()
}
