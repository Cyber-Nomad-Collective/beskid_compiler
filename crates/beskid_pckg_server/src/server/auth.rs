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
use beskid_pckg_auth::{
    ApiKeyIdentity, ApiKeyScope, AuthMode, AutheliaIdentity, AuthentikIdentity, Principal, SessionIdentity, SubjectRole,
};
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
const HEADER_AUTHENTIK_USERNAME: &str = "x-authentik-username";
const HEADER_AUTHENTIK_EMAIL: &str = "x-authentik-email";
const HEADER_AUTHENTIK_NAME: &str = "x-authentik-name";
const HEADER_AUTHENTIK_GROUPS: &str = "x-authentik-groups";
const HEADER_AUTHORIZATION: &str = "authorization";
const API_KEY_LENGTH: usize = 68;

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
        AuthMode::Authentik => authentik_identity(headers)
            .map(|identity| Principal::from_authentik(&identity, &auth.admin_group, &auth.moderator_group)),
    }
}

/// Convenience wrapper returning just the subject, for routes that only need
/// ownership comparison.
pub(crate) fn authenticated_subject(state: &AppState, headers: &HeaderMap) -> Option<String> {
    authenticated_principal(state, headers).map(|principal| principal.subject().to_owned())
}

/// Resolves the publication principal. A syntactically valid pckg bearer key
/// is verified through the active-key repository and never combines with
/// `Remote-*` headers or the mock principal. Only requests without
/// `Authorization` keep the configured session behavior, including local mock
/// mode. Publisher keys do not depend on browser-session authentication being
/// configured.
pub(crate) async fn authenticated_publisher_principal(state: &AppState, headers: &HeaderMap) -> Option<Principal> {
    match bearer_api_key(headers) {
        BearerApiKey::Absent => authenticated_principal(state, headers),
        BearerApiKey::Invalid => None,
        BearerApiKey::Token(token) => active_api_key_principal(state, token).await,
    }
}

/// Convenience wrapper for ownership-scoped publication handlers.
pub(crate) async fn authenticated_publisher_subject(state: &AppState, headers: &HeaderMap) -> Option<String> {
    authenticated_publisher_principal(state, headers).await.map(|principal| principal.subject().to_owned())
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
        AuthMode::Authentik => match authentik_identity(&headers) {
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

fn authentik_identity(headers: &HeaderMap) -> Option<AuthentikIdentity> {
    let subject = header_str(headers, HEADER_AUTHENTIK_USERNAME)?.trim().to_owned();
    if subject.is_empty() {
        return None;
    }
    Some(AuthentikIdentity {
        subject,
        email: header_str(headers, HEADER_AUTHENTIK_EMAIL).map(str::to_owned),
        display_name: header_str(headers, HEADER_AUTHENTIK_NAME).map(str::to_owned),
        groups: header_str(headers, HEADER_AUTHENTIK_GROUPS)
            .map(|value| value.split(',').map(str::trim).filter(|group| !group.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default(),
    })
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

async fn active_api_key_principal(state: &AppState, token: &str) -> Option<Principal> {
    let repository = state.api_key_auth.as_ref()?;
    let key = repository.find_active_api_key_by_token(token).await.ok()??;
    if key.revoked_at_unix_seconds.is_some()
        || !key.scopes.iter().any(|scope| scope.eq_ignore_ascii_case(ApiKeyScope::Publish.as_str()))
    {
        return None;
    }
    Some(Principal::from_api_key(ApiKeyIdentity { key_id: key.id, subject: key.subject, scopes: key.scopes }))
}

enum BearerApiKey<'a> {
    Absent,
    Invalid,
    Token(&'a str),
}

fn bearer_api_key(headers: &HeaderMap) -> BearerApiKey<'_> {
    let Some(value) = headers.get(HEADER_AUTHORIZATION) else {
        return BearerApiKey::Absent;
    };
    let Ok(value) = value.to_str() else {
        return BearerApiKey::Invalid;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return BearerApiKey::Invalid;
    };
    if token.len() != API_KEY_LENGTH
        || !token.starts_with("bpk_")
        || !token[4..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return BearerApiKey::Invalid;
    }
    BearerApiKey::Token(token)
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
