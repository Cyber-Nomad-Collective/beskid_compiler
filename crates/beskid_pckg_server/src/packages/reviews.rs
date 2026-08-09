use super::contracts::{CommunityReviewRequest, CommunityReviewResponse};
use super::mapping::{now, package_not_found, package_storage_failure, timestamp};
use super::{
    ApiErrorResponse, AppState, HeaderMap, IntoResponse, Json, PackageCommunityReview, Path, Response, State,
    StatusCode, authenticated_subject,
};

pub async fn list_community_reviews(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let Some(package) = state.packages.find_package(&name).await.ok().flatten().filter(|package| package.is_public)
    else {
        return package_not_found();
    };
    match state.packages.community_reviews(&package.id).await {
        Ok(reviews) => Json(reviews.into_iter().map(community_review_response).collect::<Vec<_>>()).into_response(),
        Err(_) => package_storage_failure(),
    }
}

pub async fn create_community_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<CommunityReviewRequest>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let Some(package) = state.packages.find_package(&name).await.ok().flatten().filter(|package| package.is_public)
    else {
        return package_not_found();
    };
    if !(1..=5).contains(&request.rating) || request.comment.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("rating must be 1-5 and comment is required")))
            .into_response();
    }
    match state.operations.block_reason(&request.comment).await {
        Ok(Some(reason)) => {
            return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new(reason))).into_response();
        }
        Ok(None) => {}
        Err(_) => return package_storage_failure(),
    }
    let now = now();
    let review = PackageCommunityReview {
        id: uuid::Uuid::new_v4().to_string(),
        package_id: package.id,
        author_subject: subject,
        rating: request.rating,
        comment: request.comment.trim().to_owned(),
        created_at_unix_seconds: now,
        updated_at_unix_seconds: now,
    };
    match state.packages.upsert_community_review(review).await {
        Ok(review) => (StatusCode::CREATED, Json(community_review_response(review))).into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid community review"))).into_response(),
    }
}

fn community_review_response(review: PackageCommunityReview) -> CommunityReviewResponse {
    CommunityReviewResponse {
        id: review.id,
        author: review.author_subject,
        rating: review.rating,
        comment: review.comment,
        created_at_utc: timestamp(review.created_at_unix_seconds),
    }
}
