use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{BoardId, CommunityError};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::SetBoardLockedRequest,
    error::{community_error, not_found, store_error},
    responses::{BoardResponse, SetBoardLockedResponse},
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn list_boards(State(state): State<CommunityState>) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(
            service
                .lock()
                .expect("community service lock is not poisoned")
                .boards()
                .into_iter()
                .map(|board| BoardResponse {
                    id: format!("{:?}", board.id),
                    title: board.title.clone(),
                    locked: board.locked,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        CommunityBackend::Sqlx(repository) => match repository.boards().await {
            Ok(boards) => Json(
                boards
                    .into_iter()
                    .map(|board| BoardResponse { id: board.id, title: board.title, locked: board.locked })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn get_board(State(state): State<CommunityState>, Path(board_id): Path<String>) -> Response {
    let Ok(board_id_value) = BoardId::new(board_id.clone()) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => service
            .lock()
            .expect("community service lock is not poisoned")
            .board(&board_id_value)
            .map(|board| {
                Json(BoardResponse { id: board_id.clone(), title: board.title.clone(), locked: board.locked })
                    .into_response()
            })
            .unwrap_or_else(not_found),
        CommunityBackend::Sqlx(repository) => match repository.board(&board_id).await {
            Ok(Some(board)) => {
                Json(BoardResponse { id: board.id, title: board.title, locked: board.locked }).into_response()
            }
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn set_board_locked(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(board_id): Path<String>,
    Json(request): Json<SetBoardLockedRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject").as_str();
    if !state.can_moderate_board(subject, &board_id).await {
        return community_error(CommunityError::Forbidden);
    }
    let message = if request.locked { "Board locked." } else { "Board unlocked." };
    match &state.backend {
        CommunityBackend::InMemory(service) => match BoardId::new(board_id).and_then(|id| {
            service
                .lock()
                .expect("community service lock is not poisoned")
                .set_board_locked(&id, request.locked)
                .map(|()| id)
        }) {
            Ok(_) => Json(SetBoardLockedResponse { success: true, message }).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => {
            let Some(mut board) = (match repository.board(&board_id).await {
                Ok(board) => board,
                Err(error) => return store_error(error),
            }) else {
                return not_found();
            };
            board.locked = request.locked;
            board.updated_at_unix_seconds = now_unix_seconds();
            match repository.create_board(board).await {
                Ok(_) => Json(SetBoardLockedResponse { success: true, message }).into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}
