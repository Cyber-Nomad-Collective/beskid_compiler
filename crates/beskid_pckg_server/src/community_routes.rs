//! HTTP adapter for the storage-independent pckg community rules.
//!
//! The parent server nests this router at `/api/community`.  It deliberately
//! derives every mutating principal from a verified pckg session instead of
//! accepting legacy pckg Identity user ids from request data.

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use beskid_pckg_auth::verify_pckg_session;
use beskid_pckg_community::{
    BoardId, Comment, CommentId, CommunityError, CommunityService, NotificationPreference, Post,
    PostId, Principal, Profile, Role, Subject, VoteValue,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
pub struct CommunityState {
    session_secret: Option<String>,
    service: Arc<Mutex<CommunityService>>,
}

impl CommunityState {
    pub fn with_session_secret(session_secret: impl Into<String>) -> Self {
        Self {
            session_secret: Some(session_secret.into()),
            service: Arc::new(Mutex::new(CommunityService::new())),
        }
    }

    #[allow(dead_code)] // Used by the direct HTTP adapter tests to seed an in-memory board.
    #[allow(dead_code)] // Integration tests use this controlled board-seeding seam.
    pub fn service(&self) -> &Arc<Mutex<CommunityService>> {
        &self.service
    }
}

pub fn router(state: CommunityState) -> Router {
    Router::new()
        .route("/profiles/{subject}", get(get_profile))
        .route("/profiles/me", put(update_my_profile))
        .route(
            "/publisher-follows/{publisher}/toggle",
            post(toggle_publisher_follow),
        )
        .route("/boards/{board_id}/posts", post(create_post))
        .route("/boards/posts/{post_id}/comments", post(create_comment))
        .route("/boards/posts/{post_id}/vote", post(vote_on_post))
        .route("/boards/comments/{comment_id}/vote", post(vote_on_comment))
        .route("/notifications", get(list_notifications))
        .route(
            "/notification-preferences",
            put(update_notification_preferences),
        )
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProfileRequest {
    display_name: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    social_links: Vec<String>,
}

#[derive(Deserialize)]
struct CreatePostRequest {
    title: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCommentRequest {
    content: String,
    parent_comment_id: Option<CommentId>,
}

#[derive(Deserialize)]
struct VoteRequest {
    value: i8,
}

#[derive(Deserialize)]
struct NotificationPreferenceRequest {
    mode: NotificationPreferenceMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum NotificationPreferenceMode {
    All,
    MentionsOnly,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FollowResponse {
    is_following: bool,
    changed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoteResponse {
    score: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostResponse {
    id: PostId,
    board_id: String,
    author: String,
    title: String,
    content: String,
    score: i32,
}

impl PostResponse {
    fn from_post(post: Post, board_id: String) -> Self {
        Self {
            id: post.id,
            board_id,
            author: post.author.as_str().to_owned(),
            title: post.title,
            content: post.content,
            score: post.score,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentResponse {
    id: CommentId,
    post_id: PostId,
    author: String,
    content: String,
    parent_comment_id: Option<CommentId>,
    score: i32,
}

impl From<Comment> for CommentResponse {
    fn from(comment: Comment) -> Self {
        Self {
            id: comment.id,
            post_id: comment.post_id,
            author: comment.author.as_str().to_owned(),
            content: comment.content,
            parent_comment_id: comment.parent_comment_id,
            score: comment.score,
        }
    }
}

async fn get_profile(State(state): State<CommunityState>, Path(subject): Path<String>) -> Response {
    let Ok(subject) = Subject::new(subject) else {
        return not_found();
    };
    let service = state
        .service
        .lock()
        .expect("community service lock is not poisoned");
    match service.profile(&subject) {
        Some(profile) => Json(profile).into_response(),
        None => not_found(),
    }
}

async fn update_my_profile(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Json(request): Json<UpdateProfileRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject")
        .clone();
    let mut profile = Profile::new(subject, request.display_name);
    profile.bio = request.bio;
    profile.social_links = request.social_links;
    state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .upsert_profile(profile.clone());
    Json(profile).into_response()
}

async fn toggle_publisher_follow(
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
    let result = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .toggle_publisher_follow(&principal, &publisher);
    match result {
        Ok(result) => Json(FollowResponse {
            is_following: result.is_following,
            changed: result.changed,
        })
        .into_response(),
        Err(error) => community_error(error),
    }
}

async fn create_post(
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
    let result = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .create_post(&principal, &board_id, request.title, request.content);
    match result {
        Ok(post) => (
            StatusCode::CREATED,
            Json(PostResponse::from_post(post, board_id_value)),
        )
            .into_response(),
        Err(error) => community_error(error),
    }
}

async fn create_comment(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(post_id): Path<PostId>,
    Json(request): Json<CreateCommentRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let result = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .create_comment(
            &principal,
            post_id,
            request.content,
            request.parent_comment_id,
        );
    match result {
        Ok(comment) => (StatusCode::CREATED, Json(CommentResponse::from(comment))).into_response(),
        Err(error) => community_error(error),
    }
}

async fn vote_on_post(
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
    let result = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .vote_on_post(&principal, post_id, vote);
    match result {
        Ok(result) => Json(VoteResponse {
            score: result.score,
        })
        .into_response(),
        Err(error) => community_error(error),
    }
}

async fn vote_on_comment(
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
    let result = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .vote_on_comment(&principal, comment_id, vote);
    match result {
        Ok(result) => Json(VoteResponse {
            score: result.score,
        })
        .into_response(),
        Err(error) => community_error(error),
    }
}

async fn list_notifications(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject");
    let notifications = state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .notifications_for(subject)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    Json(notifications).into_response()
}

async fn update_notification_preferences(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Json(request): Json<NotificationPreferenceRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let preference = match request.mode {
        NotificationPreferenceMode::All => NotificationPreference::all(),
        NotificationPreferenceMode::MentionsOnly => NotificationPreference::mentions_only(),
    };
    state
        .service
        .lock()
        .expect("community service lock is not poisoned")
        .set_notification_preference(
            principal
                .subject()
                .expect("authenticated principal has subject")
                .clone(),
            preference,
        );
    StatusCode::NO_CONTENT.into_response()
}

// Axum responses intentionally carry the complete HTTP rejection payload here;
// boxing it would only add allocation and dereferencing at every route boundary.
#[allow(clippy::result_large_err)]
fn authenticated_principal(
    state: &CommunityState,
    headers: &HeaderMap,
) -> Result<Principal, Response> {
    let Some(session_secret) = state.session_secret.as_deref() else {
        return Err(unauthorized());
    };
    let Some(session) = session_cookie(headers) else {
        return Err(unauthorized());
    };
    let identity = verify_pckg_session(session, session_secret).map_err(|_| unauthorized())?;
    let subject = Subject::new(identity.subject).map_err(|_| unauthorized())?;
    Ok(Principal::auth_hub(subject, [Role::User]))
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("pckg_session="))
}

fn vote_value(value: i8) -> VoteValue {
    match value {
        1 => VoteValue::Up,
        -1 => VoteValue::Down,
        _ => VoteValue::Clear,
    }
}

fn community_error(error: CommunityError) -> Response {
    let (status, message) = match error {
        CommunityError::BoardNotFound
        | CommunityError::PostNotFound
        | CommunityError::CommentNotFound => {
            (StatusCode::NOT_FOUND, "community resource not found")
        }
        CommunityError::Forbidden | CommunityError::BoardLocked => {
            (StatusCode::FORBIDDEN, "community action is not permitted")
        }
        CommunityError::SelfVote
        | CommunityError::InvalidBoardId
        | CommunityError::InvalidSubject => (StatusCode::BAD_REQUEST, "invalid community request"),
    };
    (status, Json(serde_json::json!({ "message": message }))).into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "message": "authentication required" })),
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "message": "community resource not found" })),
    )
        .into_response()
}
