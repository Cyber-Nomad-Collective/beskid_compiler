use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_store::{
    AdminRole, AsyncAdministrationRepository, AsyncPackageReviewRepository, PackageReviewQueueError,
    PackageReviewRequest,
};
use uuid::Uuid;

use super::contracts::{MAX_REVIEW_TEXT_BYTES, ReviewAction, ReviewResponse, ReviewSubmission};
use super::errors::{bad_request, not_found, rfc3339, unavailable};
use crate::{AppState, authenticated_subject, now_unix_seconds};

pub(crate) async fn submit_review_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<ReviewSubmission>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(Some(package)) => package,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    if package.owner_subject != subject {
        return not_found();
    }
    if !valid_review_text(&request.reason) {
        return bad_request("review reason must be non-empty and at most 4000 bytes");
    }
    let review = PackageReviewRequest {
        id: Uuid::new_v4().to_string(),
        package_id: package.id.clone(),
        requested_by_subject: subject,
        reason: request.reason.trim().to_owned(),
        status: "pending".to_owned(),
        submitted_at_unix_seconds: now_unix_seconds(),
        reviewer_subject: None,
        review_notes: None,
        reviewed_at_unix_seconds: None,
    };
    let saved = if let Some(repository) = &state.api_keys {
        match repository.submit_package_review(review).await {
            Ok(review) => review,
            Err(_) => return unavailable(),
        }
    } else {
        state.reviews.memory.lock().expect("review queue mutex is not poisoned").push(review.clone());
        review
    };
    (StatusCode::CREATED, Json(review_response(saved, package.name))).into_response()
}

pub(crate) async fn list_review_queue(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let reviews = match all_reviews(&state).await {
        Ok(reviews) => reviews,
        Err(_) => return unavailable(),
    };
    let mut response = Vec::new();
    for review in reviews {
        let package = match state.packages.find_package_by_id(&review.package_id).await {
            Ok(Some(package)) => package,
            Ok(None) => continue,
            Err(_) => return unavailable(),
        };
        if can_moderate(&state, &subject, &package.owner_subject, &package.id).await {
            response.push(review_response(review, package.name));
        }
    }
    Json(response).into_response()
}

pub(crate) async fn review_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(review_id): Path<String>,
    Json(request): Json<ReviewAction>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let Some(existing) = find_review(&state, &review_id).await else {
        return not_found();
    };
    let package = match state.packages.find_package_by_id(&existing.package_id).await {
        Ok(Some(package)) => package,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    if !can_moderate(&state, &subject, &package.owner_subject, &package.id).await {
        return not_found();
    }
    let Some(status) = canonical_action(&request.action) else {
        return bad_request("action must be approved, needs_changes, or rejected");
    };
    let notes = request.notes.and_then(|notes| (!notes.trim().is_empty()).then(|| notes.trim().to_owned()));
    if notes.as_ref().is_some_and(|notes| notes.len() > MAX_REVIEW_TEXT_BYTES) {
        return bad_request("review notes must be at most 4000 bytes");
    }
    let updated = if let Some(repository) = &state.api_keys {
        match repository.action_package_review(&review_id, status, &subject, notes, now_unix_seconds()).await {
            Ok(review) => review,
            Err(PackageReviewQueueError::NotFound) => return not_found(),
            Err(_) => return unavailable(),
        }
    } else {
        let mut reviews = state.reviews.memory.lock().expect("review queue mutex is not poisoned");
        let Some(review) = reviews.iter_mut().find(|review| review.id == review_id) else {
            return not_found();
        };
        review.status = status.to_owned();
        review.reviewer_subject = Some(subject);
        review.review_notes = notes;
        review.reviewed_at_unix_seconds = Some(now_unix_seconds());
        review.clone()
    };
    Json(review_response(updated, package.name)).into_response()
}

async fn all_reviews(state: &AppState) -> Result<Vec<PackageReviewRequest>, ()> {
    if let Some(repository) = &state.api_keys {
        repository.list_package_reviews().await.map_err(|_| ())
    } else {
        Ok(state.reviews.memory.lock().expect("review queue mutex is not poisoned").clone())
    }
}

async fn find_review(state: &AppState, id: &str) -> Option<PackageReviewRequest> {
    all_reviews(state).await.ok()?.into_iter().find(|review| review.id == id)
}

async fn can_moderate(state: &AppState, subject: &str, owner: &str, package_id: &str) -> bool {
    if subject == owner {
        return true;
    }
    let Some(repository) = &state.api_keys else {
        return false;
    };
    if repository
        .roles_for_subject(subject)
        .await
        .map(|roles| roles.iter().any(|role| matches!(role, AdminRole::Moderator | AdminRole::SuperAdmin)))
        .unwrap_or(false)
    {
        return true;
    }
    repository
        .list_resource_permissions("package", package_id)
        .await
        .map(|grants| grants.iter().any(|grant| grant.subject == subject && grant.capability == "moderate"))
        .unwrap_or(false)
}

fn review_response(review: PackageReviewRequest, package_name: String) -> ReviewResponse {
    ReviewResponse {
        id: review.id,
        package_id: review.package_id,
        package_name,
        requested_by_subject: review.requested_by_subject,
        reason: review.reason,
        status: review.status,
        submitted_at_utc: rfc3339(review.submitted_at_unix_seconds),
        reviewer_subject: review.reviewer_subject,
        review_notes: review.review_notes,
        reviewed_at_utc: review.reviewed_at_unix_seconds.map(rfc3339),
    }
}

fn canonical_action(action: &str) -> Option<&'static str> {
    match action.trim().to_ascii_lowercase().as_str() {
        "approved" => Some("approved"),
        "needs_changes" | "needschanges" => Some("needs_changes"),
        "rejected" => Some("rejected"),
        _ => None,
    }
}

fn valid_review_text(value: &str) -> bool {
    !value.trim().is_empty() && value.trim() == value && value.len() <= MAX_REVIEW_TEXT_BYTES
}
