use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use beskid_pckg_community::CommunityError;
use beskid_pckg_store::CommunityStoreError;

pub(super) fn community_error(error: CommunityError) -> Response {
    let (status, message) = match error {
        CommunityError::BoardNotFound
        | CommunityError::PostNotFound
        | CommunityError::CommentNotFound
        | CommunityError::NotificationNotFound => (StatusCode::NOT_FOUND, "community resource not found"),
        CommunityError::Forbidden | CommunityError::BoardLocked => {
            (StatusCode::FORBIDDEN, "community action is not permitted")
        }
        CommunityError::SelfVote | CommunityError::InvalidBoardId | CommunityError::InvalidSubject => {
            (StatusCode::BAD_REQUEST, "invalid community request")
        }
    };
    (status, Json(serde_json::json!({ "message": message }))).into_response()
}

pub(super) fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "message": "authentication required" }))).into_response()
}

pub(super) fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "message": "community resource not found" }))).into_response()
}

pub(super) fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "message": message }))).into_response()
}

pub(super) fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "message": "community safety policy unavailable" })))
        .into_response()
}

pub(super) fn store_error(error: CommunityStoreError) -> Response {
    match error {
        CommunityStoreError::BoardNotFound
        | CommunityStoreError::PostNotFound
        | CommunityStoreError::CommentNotFound
        | CommunityStoreError::NotificationNotFound
        | CommunityStoreError::ProfileNotFound => not_found(),
        CommunityStoreError::SelfVote => community_error(CommunityError::SelfVote),
        _ => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":"invalid community request"}))).into_response()
        }
    }
}
