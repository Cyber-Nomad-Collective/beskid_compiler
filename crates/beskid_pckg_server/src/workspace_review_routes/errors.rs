use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use super::contracts::WorkspacePublishResponse;

pub(super) fn rfc3339(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0).expect("timestamp is valid").to_rfc3339()
}

pub(super) fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({"message":"package review not found"}))).into_response()
}

pub(super) fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"message":"review persistence is unavailable"})))
        .into_response()
}

pub(super) fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":message}))).into_response()
}

pub(super) fn workspace_failure(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(WorkspacePublishResponse {
            success: false,
            message: message.into(),
            workspace_name: None,
            packages: Vec::new(),
        }),
    )
        .into_response()
}
