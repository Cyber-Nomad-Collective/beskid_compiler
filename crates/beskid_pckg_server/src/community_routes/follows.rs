use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use beskid_pckg_community::Subject;
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    error::{community_error, not_found, store_error, unauthorized},
    responses::{FollowCountResponse, FollowResponse},
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn toggle_publisher_follow(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(publisher): Path<String>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let publisher = match Subject::new(publisher) {
        Ok(publisher) => publisher,
        Err(error) => return community_error(error),
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .toggle_publisher_follow(&principal, &publisher)
        {
            Ok(result) => {
                Json(FollowResponse { is_following: result.is_following, changed: result.changed }).into_response()
            }
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .toggle_publisher_follow(
                principal.subject().expect("authenticated principal has subject").as_str(),
                publisher.as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(value) => Json(FollowResponse { is_following: value, changed: true }).into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn get_publisher_follow_count(
    State(state): State<CommunityState>,
    Path(publisher): Path<String>,
) -> Response {
    let Ok(publisher) = Subject::new(publisher) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(FollowCountResponse {
            count: service.lock().expect("community service lock is not poisoned").publisher_follow_count(&publisher),
        })
        .into_response(),
        CommunityBackend::Sqlx(repository) => match repository.publisher_follow_count(publisher.as_str()).await {
            Ok(count) => Json(FollowCountResponse { count: count as usize }).into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn get_publisher_follow_status(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(publisher): Path<String>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(subject) = principal.subject() else {
        return unauthorized();
    };
    let Ok(publisher_subject) = Subject::new(publisher.clone()) else {
        return not_found();
    };
    match &state.backend { CommunityBackend::InMemory(service) => Json(serde_json::json!({"isFollowing": service.lock().expect("community service lock is not poisoned").is_following_publisher(subject, &publisher_subject)})).into_response(), CommunityBackend::Sqlx(repository) => match repository.is_following_publisher(subject.as_str(), &publisher).await { Ok(value) => Json(serde_json::json!({"isFollowing":value})).into_response(), Err(error) => store_error(error) } }
}
pub(super) async fn toggle_package_follow(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(package_id): Path<String>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .toggle_package_follow(&principal, package_id)
        {
            Ok(result) => {
                Json(FollowResponse { is_following: result.is_following, changed: result.changed }).into_response()
            }
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => {
            let subject = principal.subject().expect("authenticated principal has subject");
            match repository.toggle_package_follow(subject.as_str(), &package_id, now_unix_seconds()).await {
                Ok(value) => Json(FollowResponse { is_following: value, changed: true }).into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}
pub(super) async fn get_package_follow_count(
    State(state): State<CommunityState>,
    Path(package_id): Path<String>,
) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(FollowCountResponse {
            count: service.lock().expect("community service lock is not poisoned").package_follow_count(&package_id),
        })
        .into_response(),
        CommunityBackend::Sqlx(repository) => match repository.package_follow_count(&package_id).await {
            Ok(count) => Json(FollowCountResponse { count: count as usize }).into_response(),
            Err(error) => store_error(error),
        },
    }
}
pub(super) async fn get_package_follow_status(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(package_id): Path<String>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject");
    match &state.backend { CommunityBackend::InMemory(service)=>Json(serde_json::json!({"isFollowing":service.lock().expect("community service lock is not poisoned").is_following_package(subject,&package_id)})).into_response(), CommunityBackend::Sqlx(repository)=>match repository.is_following_package(subject.as_str(),&package_id).await {Ok(value)=>Json(serde_json::json!({"isFollowing":value})).into_response(),Err(error)=>store_error(error)} }
}
