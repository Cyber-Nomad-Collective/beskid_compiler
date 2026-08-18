//! Request authentication for the pckg registry.
//!
//! pckg is a resource server that trusts Authelia's forward-auth session.
//! In production (`SHELL_AUTH_MODE=authelia`) the principal is derived from
//! the `Remote-User`, `Remote-Email`, `Remote-Name` and `Remote-Groups`
//! headers injected by Authelia. In local development
//! (`SHELL_AUTH_MODE=mock`) a single configurable dev principal is trusted
//! for every request so the registry can run without an Authelia instance.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use beskid_pckg_auth::{AuthMode, AutheliaIdentity, Principal, SessionIdentity, SubjectRole};
use beskid_pckg_contract::ApiErrorResponse;

use super::model::AppState;

/// The Authelia forward-auth header names. Authelia injects these on every
/// authenticated request that passes through its reverse-proxy forward-auth
/// flow; pckg trusts them because it only ever receives traffic from that
/// flow.
const HEADER_REMOTE_USER: &str = "remote-user";
const HEADER_REMOTE_EMAIL: &str = "remote-email";
const HEADER_REMOTE_NAME: &str = "remote-name";
const HEADER_REMOTE_GROUPS: &str = "remote-groups";

/// Resolves the authenticated principal for a request, or `None` when no
/// auth is configured or the request is anonymous. This is the single seam
/// every route handler uses; it never rebuilds HIR or consults a second
/// snapshot.
pub(crate) fn authenticated_principal(state: &AppState, headers: &HeaderMap) -> Option<Principal> {
    let auth = state.auth.as_ref()?;
    match auth.mode {
        AuthMode::Mock => Some(Principal::from_subject(
            auth.mock_subject.clone(),
            mock_roles(&auth.mock_groups, &auth.admin_group, &auth.moderator_group),
        )),
        AuthMode::Authelia => authelia_identity(headers)
            .map(|identity| Principal::from_authelia(&identity, &auth.admin_group, &auth.moderator_group)),
    }
}

/// Convenience wrapper returning just the subject, for routes that only need
/// ownership comparison.
pub(crate) fn authenticated_subject(state: &AppState, headers: &HeaderMap) -> Option<String> {
    authenticated_principal(state, headers).map(|principal| principal.subject().to_owned())
}

/// `/api/auth/session` handler. Returns the Authelia-projected identity so the
/// frontend can render role-gated UI without a second round-trip. Anonymous
/// requests get 401, which the frontend maps to `null`.
pub(crate) async fn read_session(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(auth) = state.auth.clone() else {
        return unauthorized_response();
    };
    let identity = match auth.mode {
        AuthMode::Mock => AutheliaIdentity {
            subject: auth.mock_subject.clone(),
            email: None,
            display_name: Some(auth.mock_subject.clone()),
            groups: auth.mock_groups.clone(),
        },
        AuthMode::Authelia => match authelia_identity(&headers) {
            Some(identity) => identity,
            None => return unauthorized_response(),
        },
    };
    Json(SessionIdentity {
        subject: identity.subject,
        email: identity.email,
        display_name: identity.display_name,
        groups: identity.groups,
    })
    .into_response()
}

fn authelia_identity(headers: &HeaderMap) -> Option<AutheliaIdentity> {
    let subject = header_str(headers, HEADER_REMOTE_USER)?.trim().to_owned();
    if subject.is_empty() {
        return None;
    }
    Some(AutheliaIdentity {
        subject,
        email: header_str(headers, HEADER_REMOTE_EMAIL).map(str::to_owned),
        display_name: header_str(headers, HEADER_REMOTE_NAME).map(str::to_owned),
        groups: header_str(headers, HEADER_REMOTE_GROUPS)
            .map(|value| value.split(',').map(str::trim).filter(|group| !group.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default(),
    })
}

fn mock_roles(mock_groups: &[String], admin_group: &str, moderator_group: &str) -> Vec<SubjectRole> {
    let mut roles = vec![SubjectRole::User];
    for group in mock_groups {
        if group == admin_group {
            roles.push(SubjectRole::SuperAdmin);
        } else if group == moderator_group {
            roles.push(SubjectRole::Moderator);
        }
    }
    roles
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::trim).filter(|value| !value.is_empty())
}

pub(crate) fn unauthorized_response() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, Json(ApiErrorResponse::new("authentication required"))).into_response()
}

pub(crate) fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs() as i64
}

pub(crate) fn format_timestamp(unix_seconds: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_seconds, 0)
        .map(|timestamp| timestamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}
