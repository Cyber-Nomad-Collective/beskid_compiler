use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect},
};
use beskid_pckg_auth::{AuthHubIdentity, HandoffRequest, issue_pckg_session, verify_pckg_session};
use beskid_pckg_contract::{ApiErrorResponse, SessionResponse};
use serde::Deserialize;

use super::model::AppState;

#[derive(Debug, Deserialize)]
struct AuthHubFinishQuery {
    handoff: Option<String>,
}

pub(super) async fn auth_hub_finish(
    State(state): State<AppState>,
    Query(query): Query<AuthHubFinishQuery>,
) -> impl IntoResponse {
    let Some(handoff) = query.handoff.filter(|value| !value.trim().is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("handoff is required"))).into_response();
    };
    let Some(auth) = state.auth else {
        return unauthorized_response();
    };
    let identity = match auth.handoff_verifier.verify(HandoffRequest { app: "pckg".to_owned(), handoff }) {
        Ok(identity) => identity,
        Err(_) => return invalid_handoff_response(),
    };
    let session = match issue_pckg_session(&identity, &auth.session_secret) {
        Ok(session) => session,
        Err(_) => return invalid_handoff_response(),
    };
    let secure = if auth.secure_cookies { "; Secure" } else { "" };
    let cookie = format!("pckg_session={session}; HttpOnly; Path=/; SameSite=Lax; Max-Age=28800{secure}");
    let mut response = Redirect::to("/dashboard/packages/my").into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie.parse().expect("session cookie uses valid header characters"));
    response
}

pub(super) async fn read_session(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(auth) = state.auth else {
        return unauthorized_response();
    };
    let Some(session) = session_cookie(&headers) else {
        return unauthorized_response();
    };
    match verify_pckg_session(session, &auth.session_secret) {
        Ok(identity) => Json(session_response(identity)).into_response(),
        Err(_) => unauthorized_response(),
    }
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("pckg_session="))
}

fn session_response(identity: AuthHubIdentity) -> SessionResponse {
    SessionResponse {
        subject: identity.subject,
        github_login: identity.github_login,
        hub_session_id: identity.hub_session_id,
    }
}

pub(crate) fn authenticated_subject(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let auth = state.auth.as_ref()?;
    let session = session_cookie(headers)?;
    verify_pckg_session(session, &auth.session_secret).ok().map(|identity| identity.subject)
}

pub(crate) fn unauthorized_response() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, Json(ApiErrorResponse::new("authentication required"))).into_response()
}

fn invalid_handoff_response() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, Json(ApiErrorResponse::new("invalid handoff"))).into_response()
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
