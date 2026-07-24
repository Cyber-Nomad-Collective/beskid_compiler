//! Owner-scoped API-key management. Raw tokens are returned exactly once by
//! create and are never included in list or revoke responses.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_auth::ApiKeyScope;
use beskid_pckg_contract::ApiErrorResponse;
use beskid_pckg_store::{AsyncApiKeyRepository, NewApiKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, authenticated_subject, unauthorized_response};

#[derive(Serialize)]
struct ApiKeyResponse {
    id: String,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    #[serde(rename = "createdAtUtc")]
    created_at_utc: String,
    #[serde(rename = "revokedAtUtc")]
    revoked_at_utc: Option<String>,
}

#[derive(Serialize)]
struct CreatedApiKeyResponse {
    key: ApiKeyResponse,
    #[serde(rename = "plainTextKey")]
    plain_text_key: String,
}

#[derive(Deserialize)]
pub(crate) struct CreateApiKeyRequest {
    name: String,
    scopes: Vec<String>,
}

pub(crate) async fn list_api_keys(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    match repository.list_api_keys(&subject).await {
        Ok(keys) => Json(keys.into_iter().map(api_key_response).collect::<Vec<_>>()).into_response(),
        Err(_) => unavailable(),
    }
}

pub(crate) async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateApiKeyRequest>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    let scopes = match normalized_scopes(request.scopes) {
        Some(scopes) => scopes,
        None => return bad_request("scopes must contain read and/or publish"),
    };
    let key_id = Uuid::new_v4();
    // The first UUID is also the non-secret list prefix; the second keeps the
    // issued bearer credential high entropy even if its prefix is displayed.
    let token = format!("bpk_{}{}", key_id.simple(), Uuid::new_v4().simple());
    match repository
        .create_api_key(NewApiKey {
            id: key_id.to_string(),
            subject,
            label: request.name,
            scopes,
            raw_token: token.clone(),
            now_unix_seconds: now(),
        })
        .await
    {
        Ok(key) => {
            (StatusCode::CREATED, Json(CreatedApiKeyResponse { key: api_key_response(key), plain_text_key: token }))
                .into_response()
        }
        Err(_) => bad_request("invalid API key request"),
    }
}

pub(crate) async fn revoke_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return unauthorized_response();
    };
    let Some(repository) = state.api_keys else {
        return unavailable();
    };
    match repository.revoke_api_key(&id, &subject, now()).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        // Preserve ownership: unknown, malformed, and foreign ids are all hidden.
        Ok(false) | Err(_) => (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("API key not found"))).into_response(),
    }
}

fn api_key_response(key: beskid_pckg_store::ApiKey) -> ApiKeyResponse {
    ApiKeyResponse {
        prefix: format!("bpk_{}", &key.id.replace('-', "")[..8]),
        id: key.id,
        name: key.label,
        scopes: key.scopes,
        created_at_utc: rfc3339(key.created_at_unix_seconds),
        revoked_at_utc: key.revoked_at_unix_seconds.map(rfc3339),
    }
}
fn rfc3339(unix_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(unix_seconds, 0).expect("repository timestamps are valid").to_rfc3339()
}
fn normalized_scopes(scopes: Vec<String>) -> Option<Vec<String>> {
    let mut result = Vec::new();
    for scope in scopes {
        let parsed = match scope.to_ascii_lowercase().as_str() {
            "read" => ApiKeyScope::Read,
            "publish" => ApiKeyScope::Publish,
            _ => return None,
        };
        let value = parsed.as_str().to_owned();
        if !result.contains(&value) {
            result.push(value);
        }
    }
    (!result.is_empty()).then_some(result)
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs() as i64
}
fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new(message))).into_response()
}
fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(ApiErrorResponse::new("API-key persistence is not configured")))
        .into_response()
}
