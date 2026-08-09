use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{CommentId, PostId, VoteValue};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::VoteRequest,
    error::{community_error, store_error},
    responses::VoteResponse,
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn vote_on_post(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(post_id): Path<PostId>,
    Json(request): Json<VoteRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let vote = vote_value(request.value);
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .vote_on_post(&principal, post_id, vote)
        {
            Ok(result) => Json(VoteResponse { score: result.score }).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .vote_on_post(
                post_id as i64,
                principal.subject().expect("authenticated principal has subject").as_str(),
                store_vote(request.value),
                now_unix_seconds(),
            )
            .await
        {
            Ok(score) => Json(VoteResponse { score }).into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn vote_on_comment(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(comment_id): Path<CommentId>,
    Json(request): Json<VoteRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let vote = vote_value(request.value);
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .vote_on_comment(&principal, comment_id, vote)
        {
            Ok(result) => Json(VoteResponse { score: result.score }).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .vote_on_comment(
                comment_id as i64,
                principal.subject().expect("authenticated principal has subject").as_str(),
                store_vote(request.value),
                now_unix_seconds(),
            )
            .await
        {
            Ok(score) => Json(VoteResponse { score }).into_response(),
            Err(error) => store_error(error),
        },
    }
}

fn vote_value(value: i8) -> VoteValue {
    match value {
        1 => VoteValue::Up,
        -1 => VoteValue::Down,
        _ => VoteValue::Clear,
    }
}

fn store_vote(value: i8) -> beskid_pckg_store::CommunityVote {
    match value {
        1 => beskid_pckg_store::CommunityVote::Up,
        -1 => beskid_pckg_store::CommunityVote::Down,
        _ => beskid_pckg_store::CommunityVote::Clear,
    }
}
