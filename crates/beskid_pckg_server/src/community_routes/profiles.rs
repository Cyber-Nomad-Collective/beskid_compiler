use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{Profile, Subject};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::UpdateProfileRequest,
    error::{not_found, store_error},
    responses::profile_response_from_store,
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn get_profile(State(state): State<CommunityState>, Path(subject): Path<String>) -> Response {
    let Ok(subject) = Subject::new(subject) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            match service.lock().expect("community service lock is not poisoned").profile(&subject) {
                Some(profile) => Json(profile).into_response(),
                None => not_found(),
            }
        }
        CommunityBackend::Sqlx(repository) => match repository.profile(subject.as_str()).await {
            Ok(Some(profile)) => Json(profile_response_from_store(profile)).into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn get_my_profile(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            match service.lock().expect("community service lock is not poisoned").profile(subject) {
                Some(profile) => Json(profile).into_response(),
                None => not_found(),
            }
        }
        CommunityBackend::Sqlx(repository) => match repository.profile(subject.as_str()).await {
            Ok(Some(profile)) => Json(profile_response_from_store(profile)).into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn update_my_profile(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Json(request): Json<UpdateProfileRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject").clone();
    let mut profile = Profile::new(subject.clone(), request.display_name.clone());
    profile.bio = request.bio.clone();
    profile.social_links = request.social_links.clone();
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            service.lock().expect("community service lock is not poisoned").upsert_profile(profile.clone());
            Json(profile).into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository
            .upsert_profile(beskid_pckg_store::CommunityProfile {
                subject: subject.as_str().to_owned(),
                display_name: request.display_name,
                bio: request.bio,
                social_links_json: serde_json::to_string(&request.social_links).unwrap_or_else(|_| "[]".into()),
                is_publisher_verified: false,
                updated_at_unix_seconds: now_unix_seconds(),
            })
            .await
        {
            Ok(profile) => Json(profile_response_from_store(profile)).into_response(),
            Err(error) => store_error(error),
        },
    }
}
