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
    routing::{get, post},
};
use beskid_pckg_auth::verify_pckg_session;
use beskid_pckg_community::{
    BoardId, Comment, CommentId, CommunityError, CommunityService, NotificationPreference, Post,
    PostId, Principal, Profile, Role, Subject, VoteValue,
};
use beskid_pckg_store::{AsyncCommunityRepository, CommunityStoreError, SqlxCommunityRepository};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct CommunityState {
    session_secret: Option<String>,
    backend: CommunityBackend,
}

/// The deliberately small profile projection exposed to registry-owned
/// catalog routes.  It contains only persisted community profile fields; in
/// particular it never invents a GitHub login from an Auth Hub subject.
#[derive(Clone, Debug)]
pub(crate) struct CatalogProfile {
    pub subject: String,
    pub display_name: String,
    pub bio: String,
    pub social_links: Vec<String>,
    pub is_publisher_verified: bool,
}

#[derive(Clone)]
enum CommunityBackend {
    InMemory(Arc<Mutex<CommunityService>>),
    Sqlx(Arc<SqlxCommunityRepository>),
}

impl Default for CommunityState {
    fn default() -> Self {
        Self {
            session_secret: None,
            backend: CommunityBackend::InMemory(Arc::new(Mutex::new(CommunityService::new()))),
        }
    }
}

impl CommunityState {
    pub fn with_session_secret(session_secret: impl Into<String>) -> Self {
        Self {
            session_secret: Some(session_secret.into()),
            backend: CommunityBackend::InMemory(Arc::new(Mutex::new(CommunityService::new()))),
        }
    }

    pub fn with_sqlx_session_secret(
        session_secret: impl Into<String>,
        repository: Arc<SqlxCommunityRepository>,
    ) -> Self {
        Self {
            session_secret: Some(session_secret.into()),
            backend: CommunityBackend::Sqlx(repository),
        }
    }

    #[allow(dead_code)] // Used by the direct HTTP adapter tests to seed an in-memory board.
    #[allow(dead_code)] // Integration tests use this controlled board-seeding seam.
    pub fn service(&self) -> &Arc<Mutex<CommunityService>> {
        match &self.backend {
            CommunityBackend::InMemory(service) => service,
            CommunityBackend::Sqlx(_) => {
                panic!("SQL-backed community state does not expose an in-memory service")
            }
        }
    }

    pub(crate) async fn profile_for_catalog(
        &self,
        subject: &str,
    ) -> Result<Option<CatalogProfile>, CommunityStoreError> {
        let Ok(subject) = Subject::new(subject.to_owned()) else {
            return Ok(None);
        };
        match &self.backend {
            CommunityBackend::InMemory(service) => Ok(service
                .lock()
                .expect("community service lock is not poisoned")
                .profile(&subject)
                .map(|profile| CatalogProfile {
                    subject: profile.subject.as_str().to_owned(),
                    display_name: profile.display_name.clone(),
                    bio: profile.bio.clone(),
                    social_links: profile.social_links.clone(),
                    is_publisher_verified: profile.is_publisher_verified,
                })),
            CommunityBackend::Sqlx(repository) => {
                repository.profile(subject.as_str()).await.map(|profile| {
                    profile.map(|profile| CatalogProfile {
                        subject: profile.subject,
                        display_name: profile.display_name,
                        bio: profile.bio,
                        social_links: serde_json::from_str(&profile.social_links_json)
                            .unwrap_or_default(),
                        is_publisher_verified: profile.is_publisher_verified,
                    })
                })
            }
        }
    }
}

pub fn router(state: CommunityState) -> Router {
    Router::new()
        .route("/profiles/me", get(get_my_profile).put(update_my_profile))
        .route("/profiles/{subject}", get(get_profile))
        .route("/boards", get(list_boards))
        .route("/boards/{board_id}", get(get_board))
        .route(
            "/boards/{board_id}/posts",
            get(list_posts).post(create_post),
        )
        .route("/boards/posts/{post_id}", get(get_post))
        .route(
            "/boards/posts/{post_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/publisher-follows/{publisher}/toggle",
            post(toggle_publisher_follow),
        )
        .route(
            "/publisher-follows/{publisher}/count",
            get(get_publisher_follow_count),
        )
        .route(
            "/publisher-follows/{publisher}/status",
            get(get_publisher_follow_status),
        )
        .route(
            "/package-follows/{package_id}/toggle",
            post(toggle_package_follow),
        )
        .route(
            "/package-follows/{package_id}/count",
            get(get_package_follow_count),
        )
        .route(
            "/package-follows/{package_id}/status",
            get(get_package_follow_status),
        )
        .route("/boards/posts/{post_id}/vote", post(vote_on_post))
        .route("/boards/comments/{comment_id}/vote", post(vote_on_comment))
        .route("/notifications", get(list_notifications))
        .route(
            "/notifications/{notification_id}/read",
            post(mark_notification_read),
        )
        .route(
            "/notification-preferences",
            get(get_notification_preferences).put(update_notification_preferences),
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
struct FollowCountResponse {
    count: usize,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardResponse {
    id: String,
    title: String,
    locked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationPreferenceResponse {
    mention_enabled: bool,
    reply_enabled: bool,
    followed_publisher_post_enabled: bool,
    moderation_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationResponse {
    id: i64,
    recipient: String,
    scope: String,
    actor: String,
    post_id: Option<i64>,
    comment_id: Option<i64>,
    is_read: bool,
}

async fn list_boards(State(state): State<CommunityState>) -> Response {
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
                    .map(|board| BoardResponse {
                        id: board.id,
                        title: board.title,
                        locked: board.locked,
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => store_error(error),
        },
    }
}

async fn get_board(State(state): State<CommunityState>, Path(board_id): Path<String>) -> Response {
    let Ok(board_id_value) = BoardId::new(board_id.clone()) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => service
            .lock()
            .expect("community service lock is not poisoned")
            .board(&board_id_value)
            .map(|board| {
                Json(BoardResponse {
                    id: board_id.clone(),
                    title: board.title.clone(),
                    locked: board.locked,
                })
                .into_response()
            })
            .unwrap_or_else(not_found),
        CommunityBackend::Sqlx(repository) => match repository.board(&board_id).await {
            Ok(Some(board)) => Json(BoardResponse {
                id: board.id,
                title: board.title,
                locked: board.locked,
            })
            .into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}

async fn list_posts(State(state): State<CommunityState>, Path(board_id): Path<String>) -> Response {
    let Ok(board_id_value) = BoardId::new(board_id.clone()) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let service = service
                .lock()
                .expect("community service lock is not poisoned");
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
                Ok(posts) => Json(
                    posts
                        .into_iter()
                        .map(post_response_from_store)
                        .collect::<Vec<_>>(),
                )
                .into_response(),
                Err(error) => store_error(error),
            },
        },
    }
}

async fn get_post(State(state): State<CommunityState>, Path(post_id): Path<PostId>) -> Response {
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

async fn list_comments(
    State(state): State<CommunityState>,
    Path(post_id): Path<PostId>,
) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let service = service
                .lock()
                .expect("community service lock is not poisoned");
            if service.post(post_id).is_none() {
                return not_found();
            }
            Json(
                service
                    .comments_for_post(post_id)
                    .into_iter()
                    .cloned()
                    .map(CommentResponse::from)
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository.post(post_id as i64).await {
            Ok(None) => not_found(),
            Err(error) => store_error(error),
            Ok(Some(_)) => match repository.comments_for_post(post_id as i64).await {
                Ok(comments) => Json(
                    comments
                        .into_iter()
                        .map(comment_response_from_store)
                        .collect::<Vec<_>>(),
                )
                .into_response(),
                Err(error) => store_error(error),
            },
        },
    }
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .profile(&subject)
        {
            Some(profile) => Json(profile).into_response(),
            None => not_found(),
        },
        CommunityBackend::Sqlx(repository) => match repository.profile(subject.as_str()).await {
            Ok(Some(profile)) => Json(profile_response_from_store(profile)).into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
    }
}

async fn get_my_profile(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .profile(subject)
        {
            Some(profile) => Json(profile).into_response(),
            None => not_found(),
        },
        CommunityBackend::Sqlx(repository) => match repository.profile(subject.as_str()).await {
            Ok(Some(profile)) => Json(profile_response_from_store(profile)).into_response(),
            Ok(None) => not_found(),
            Err(error) => store_error(error),
        },
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
    let mut profile = Profile::new(subject.clone(), request.display_name.clone());
    profile.bio = request.bio.clone();
    profile.social_links = request.social_links.clone();
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            service
                .lock()
                .expect("community service lock is not poisoned")
                .upsert_profile(profile.clone());
            Json(profile).into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository
            .upsert_profile(beskid_pckg_store::CommunityProfile {
                subject: subject.as_str().to_owned(),
                display_name: request.display_name,
                bio: request.bio,
                social_links_json: serde_json::to_string(&request.social_links)
                    .unwrap_or_else(|_| "[]".into()),
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .toggle_publisher_follow(&principal, &publisher)
        {
            Ok(result) => Json(FollowResponse {
                is_following: result.is_following,
                changed: result.changed,
            })
            .into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .toggle_publisher_follow(
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
                publisher.as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(value) => Json(FollowResponse {
                is_following: value,
                changed: true,
            })
            .into_response(),
            Err(error) => store_error(error),
        },
    }
}

async fn get_publisher_follow_count(
    State(state): State<CommunityState>,
    Path(publisher): Path<String>,
) -> Response {
    let Ok(publisher) = Subject::new(publisher) else {
        return not_found();
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(FollowCountResponse {
            count: service
                .lock()
                .expect("community service lock is not poisoned")
                .publisher_follow_count(&publisher),
        })
        .into_response(),
        CommunityBackend::Sqlx(repository) => {
            match repository.publisher_follow_count(publisher.as_str()).await {
                Ok(count) => Json(FollowCountResponse {
                    count: count as usize,
                })
                .into_response(),
                Err(error) => store_error(error),
            }
        }
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .create_post(&principal, &board_id, request.title, request.content)
        {
            Ok(post) => (
                StatusCode::CREATED,
                Json(PostResponse::from_post(post, board_id_value)),
            )
                .into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .create_post(
                &board_id_value,
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .create_comment(
                &principal,
                post_id,
                request.content,
                request.parent_comment_id,
            ) {
            Ok(comment) => {
                (StatusCode::CREATED, Json(CommentResponse::from(comment))).into_response()
            }
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .create_comment(
                post_id as i64,
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
                &request.content,
                request.parent_comment_id.map(|id| id as i64),
                now_unix_seconds(),
            )
            .await
        {
            Ok(comment) => (
                StatusCode::CREATED,
                Json(comment_response_from_store(comment)),
            )
                .into_response(),
            Err(error) => store_error(error),
        },
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .vote_on_post(&principal, post_id, vote)
        {
            Ok(result) => Json(VoteResponse {
                score: result.score,
            })
            .into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .vote_on_post(
                post_id as i64,
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
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
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .vote_on_comment(&principal, comment_id, vote)
        {
            Ok(result) => Json(VoteResponse {
                score: result.score,
            })
            .into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .vote_on_comment(
                comment_id as i64,
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
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

async fn list_notifications(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(
            service
                .lock()
                .expect("community service lock is not poisoned")
                .notifications_for(subject)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_response(),
        CommunityBackend::Sqlx(repository) => {
            match repository.list_notifications(subject.as_str()).await {
                Ok(notifications) => Json(
                    notifications
                        .into_iter()
                        .map(|notification| NotificationResponse {
                            id: notification.id,
                            recipient: notification.recipient_subject,
                            scope: notification.scope,
                            actor: notification.actor_subject,
                            post_id: notification.post_id,
                            comment_id: notification.comment_id,
                            is_read: notification.read_at_unix_seconds.is_some(),
                        })
                        .collect::<Vec<_>>(),
                )
                .into_response(),
                Err(error) => store_error(error),
            }
        }
    }
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
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let preference = match request.mode {
                NotificationPreferenceMode::All => NotificationPreference::all(),
                NotificationPreferenceMode::MentionsOnly => NotificationPreference::mentions_only(),
            };
            service
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
        CommunityBackend::Sqlx(repository) => {
            let preference = match request.mode {
                NotificationPreferenceMode::All => {
                    beskid_pckg_store::CommunityNotificationPreference::default()
                }
                NotificationPreferenceMode::MentionsOnly => {
                    beskid_pckg_store::CommunityNotificationPreference {
                        mention_enabled: true,
                        reply_enabled: false,
                        followed_publisher_post_enabled: false,
                        moderation_enabled: false,
                    }
                }
            };
            match repository
                .set_notification_preference(
                    principal
                        .subject()
                        .expect("authenticated principal has subject")
                        .as_str(),
                    preference,
                    now_unix_seconds(),
                )
                .await
            {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(error) => store_error(error),
            }
        }
    }
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

fn store_vote(value: i8) -> beskid_pckg_store::CommunityVote {
    match value {
        1 => beskid_pckg_store::CommunityVote::Up,
        -1 => beskid_pckg_store::CommunityVote::Down,
        _ => beskid_pckg_store::CommunityVote::Clear,
    }
}

fn community_error(error: CommunityError) -> Response {
    let (status, message) = match error {
        CommunityError::BoardNotFound
        | CommunityError::PostNotFound
        | CommunityError::CommentNotFound
        | CommunityError::NotificationNotFound => {
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

fn post_response_from_store(post: beskid_pckg_store::CommunityPost) -> PostResponse {
    PostResponse {
        id: post.id as PostId,
        board_id: post.board_id,
        author: post.author_subject,
        title: post.title,
        content: post.content,
        score: post.score,
    }
}

fn comment_response_from_store(comment: beskid_pckg_store::CommunityComment) -> CommentResponse {
    CommentResponse {
        id: comment.id as CommentId,
        post_id: comment.post_id as PostId,
        author: comment.author_subject,
        content: comment.content,
        parent_comment_id: comment.parent_comment_id.map(|id| id as CommentId),
        score: comment.score,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreProfileResponse {
    subject: String,
    display_name: String,
    bio: String,
    social_links: Vec<String>,
    is_publisher_verified: bool,
}
fn profile_response_from_store(
    profile: beskid_pckg_store::CommunityProfile,
) -> StoreProfileResponse {
    StoreProfileResponse {
        subject: profile.subject,
        display_name: profile.display_name,
        bio: profile.bio,
        social_links: serde_json::from_str(&profile.social_links_json).unwrap_or_default(),
        is_publisher_verified: profile.is_publisher_verified,
    }
}
fn store_error(error: CommunityStoreError) -> Response {
    match error {
        CommunityStoreError::BoardNotFound
        | CommunityStoreError::PostNotFound
        | CommunityStoreError::CommentNotFound
        | CommunityStoreError::NotificationNotFound
        | CommunityStoreError::ProfileNotFound => not_found(),
        CommunityStoreError::SelfVote => community_error(CommunityError::SelfVote),
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"message":"invalid community request"})),
        )
            .into_response(),
    }
}
fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

async fn get_publisher_follow_status(
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
async fn toggle_package_follow(
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
            Ok(result) => Json(FollowResponse {
                is_following: result.is_following,
                changed: result.changed,
            })
            .into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => {
            let subject = principal
                .subject()
                .expect("authenticated principal has subject");
            match repository
                .toggle_package_follow(subject.as_str(), &package_id, now_unix_seconds())
                .await
            {
                Ok(value) => Json(FollowResponse {
                    is_following: value,
                    changed: true,
                })
                .into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}
async fn get_package_follow_count(
    State(state): State<CommunityState>,
    Path(package_id): Path<String>,
) -> Response {
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(FollowCountResponse {
            count: service
                .lock()
                .expect("community service lock is not poisoned")
                .package_follow_count(&package_id),
        })
        .into_response(),
        CommunityBackend::Sqlx(repository) => {
            match repository.package_follow_count(&package_id).await {
                Ok(count) => Json(FollowCountResponse {
                    count: count as usize,
                })
                .into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}
async fn get_package_follow_status(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(package_id): Path<String>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject");
    match &state.backend { CommunityBackend::InMemory(service)=>Json(serde_json::json!({"isFollowing":service.lock().expect("community service lock is not poisoned").is_following_package(subject,&package_id)})).into_response(), CommunityBackend::Sqlx(repository)=>match repository.is_following_package(subject.as_str(),&package_id).await {Ok(value)=>Json(serde_json::json!({"isFollowing":value})).into_response(),Err(error)=>store_error(error)} }
}
async fn get_notification_preferences(
    State(state): State<CommunityState>,
    headers: HeaderMap,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let subject = principal
        .subject()
        .expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let preference = service
                .lock()
                .expect("community service lock is not poisoned")
                .notification_preference(subject);
            Json(serde_json::json!({"mentionsOnly": preference.allows(beskid_pckg_community::NotificationScope::Mention) && !preference.allows(beskid_pckg_community::NotificationScope::Reply)})).into_response()
        }
        CommunityBackend::Sqlx(repository) => {
            match repository.notification_preference(subject.as_str()).await {
                Ok(value) => Json(NotificationPreferenceResponse {
                    mention_enabled: value.mention_enabled,
                    reply_enabled: value.reply_enabled,
                    followed_publisher_post_enabled: value.followed_publisher_post_enabled,
                    moderation_enabled: value.moderation_enabled,
                })
                .into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}
async fn mark_notification_read(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(notification_id): Path<u64>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .mark_notification_read(&principal, notification_id)
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .mark_notification_read(
                notification_id as i64,
                principal
                    .subject()
                    .expect("authenticated principal has subject")
                    .as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => store_error(error),
        },
    }
}
