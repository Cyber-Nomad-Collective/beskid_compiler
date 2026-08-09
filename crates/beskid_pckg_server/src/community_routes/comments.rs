use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{CommentId, PostId};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::CreateCommentRequest,
    error::{bad_request, community_error, not_found, store_error},
    responses::{CommentResponse, comment_response_from_store},
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn list_comments(State(state): State<CommunityState>, Path(post_id): Path<PostId>) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let service = service.lock().expect("community service lock is not poisoned");
            if service.post(post_id).is_none() {
                return not_found();
            }
            Json(service.comments_for_post(post_id).into_iter().cloned().map(CommentResponse::from).collect::<Vec<_>>())
                .into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository.post(post_id as i64).await {
            Ok(None) => not_found(),
            Err(error) => store_error(error),
            Ok(Some(_)) => match repository.comments_for_post(post_id as i64).await {
                Ok(comments) => {
                    Json(comments.into_iter().map(comment_response_from_store).collect::<Vec<_>>()).into_response()
                }
                Err(error) => store_error(error),
            },
        },
    }
}

pub(super) async fn create_comment(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(post_id): Path<PostId>,
    Json(request): Json<CreateCommentRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if let Some(message) = match state.blocked_link_reason(&request.content).await {
        Ok(reason) => reason,
        Err(response) => return response,
    } {
        return bad_request(message);
    }
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .create_comment(&principal, post_id, request.content, request.parent_comment_id)
        {
            Ok(comment) => (StatusCode::CREATED, Json(CommentResponse::from(comment))).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .create_comment(
                post_id as i64,
                principal.subject().expect("authenticated principal has subject").as_str(),
                &request.content,
                request.parent_comment_id.map(|id| id as i64),
                now_unix_seconds(),
            )
            .await
        {
            Ok(comment) => (StatusCode::CREATED, Json(comment_response_from_store(comment))).into_response(),
            Err(error) => store_error(error),
        },
    }
}
