use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{BoardId, PostId};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::CreatePostRequest,
    error::{bad_request, community_error, not_found, store_error},
    responses::{PostResponse, post_response_from_store},
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn list_posts(State(state): State<CommunityState>, Path(board_id): Path<String>) -> Response {
    let Ok(board_id_value) = BoardId::new(board_id.clone()) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let service = service.lock().expect("community service lock is not poisoned");
            if service.board(&board_id_value).is_none() {
                return not_found();
            }
            Json(
                service
                    .posts_for_board(&board_id_value)
                    .into_iter()
                    .cloned()
                    .map(|post| PostResponse::from_post(post, board_id.clone()))
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository.board(&board_id).await {
            Ok(None) => not_found(),
            Err(error) => store_error(error),
            Ok(Some(_)) => match repository.posts_for_board(&board_id).await {
                Ok(posts) => Json(posts.into_iter().map(post_response_from_store).collect::<Vec<_>>()).into_response(),
                Err(error) => store_error(error),
            },
        },
    }
}

pub(super) async fn get_post(State(state): State<CommunityState>, Path(post_id): Path<PostId>) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => service
            .lock()
            .expect("community service lock is not poisoned")
            .post(post_id)
            .cloned()
            .map(|post| {
                let board_id = format!("{:?}", post.board_id);
                Json(PostResponse::from_post(post, board_id)).into_response()
            })
            .unwrap_or_else(not_found),
        CommunityBackend::Sqlx(repository) => match repository.post(post_id as i64).await {
            Ok(Some(post)) => Json(post_response_from_store(post)).into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}
pub(super) async fn create_post(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(board_id): Path<String>,
    Json(request): Json<CreatePostRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let board_id_value = board_id.clone();
    let board_id = match BoardId::new(board_id) {
        Ok(board_id) => board_id,
        Err(error) => return community_error(error),
    };
    if let Some(message) = match state.blocked_link_reason(&format!("{}\n{}", request.title, request.content)).await {
        Ok(reason) => reason,
        Err(response) => return response,
    } {
        return bad_request(message);
    }
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .create_post(&principal, &board_id, request.title, request.content)
        {
            Ok(post) => (StatusCode::CREATED, Json(PostResponse::from_post(post, board_id_value))).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .create_post(
                &board_id_value,
                principal.subject().expect("authenticated principal has subject").as_str(),
                &request.title,
                &request.content,
                now_unix_seconds(),
            )
            .await
        {
            Ok(post) => (StatusCode::CREATED, Json(post_response_from_store(post))).into_response(),
            Err(error) => store_error(error),
        },
    }
}
