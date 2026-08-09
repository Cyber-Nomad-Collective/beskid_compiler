//! HTTP adapter for the storage-independent pckg community rules.
//!
//! The parent server nests this router at `/api/community`.  It deliberately
//! derives every mutating principal from a verified pckg session instead of
//! accepting legacy pckg Identity user ids from request data.

mod auth;
mod boards;
mod comments;
mod contracts;
mod error;
mod follows;
mod notifications;
mod posts;
mod profiles;
mod responses;
mod state;
mod votes;

use axum::{
    Router,
    routing::{get, post},
};

use self::{
    boards::{get_board, list_boards, set_board_locked},
    comments::{create_comment, list_comments},
    follows::{
        get_package_follow_count, get_package_follow_status, get_publisher_follow_count, get_publisher_follow_status,
        toggle_package_follow, toggle_publisher_follow,
    },
    notifications::{
        execute_notification_action, get_notification_preferences, list_notifications, mark_all_notifications_read,
        mark_notification_read, send_test_notification, update_notification_preferences,
    },
    posts::{create_post, get_post, list_posts},
    profiles::{get_my_profile, get_profile, update_my_profile},
    votes::{vote_on_comment, vote_on_post},
};

pub use self::state::CommunityState;
pub(crate) use self::state::{CatalogProfile, CommunityLinkPolicy, CommunityLinkPolicyFuture};

pub fn router(state: CommunityState) -> Router {
    Router::new()
        .route("/profiles/me", get(get_my_profile).put(update_my_profile))
        .route("/profiles/{subject}", get(get_profile))
        .route("/boards", get(list_boards))
        .route("/boards/{board_id}", get(get_board))
        .route("/boards/{board_id}/moderation/lock", post(set_board_locked))
        .route("/boards/{board_id}/posts", get(list_posts).post(create_post))
        .route("/boards/posts/{post_id}", get(get_post))
        .route("/boards/posts/{post_id}/comments", get(list_comments).post(create_comment))
        .route("/publisher-follows/{publisher}/toggle", post(toggle_publisher_follow))
        .route("/publisher-follows/{publisher}/count", get(get_publisher_follow_count))
        .route("/publisher-follows/{publisher}/status", get(get_publisher_follow_status))
        .route("/package-follows/{package_id}/toggle", post(toggle_package_follow))
        .route("/package-follows/{package_id}/count", get(get_package_follow_count))
        .route("/package-follows/{package_id}/status", get(get_package_follow_status))
        .route("/boards/posts/{post_id}/vote", post(vote_on_post))
        .route("/boards/comments/{comment_id}/vote", post(vote_on_comment))
        .route("/notifications", get(list_notifications))
        .route("/notifications/{notification_id}/read", post(mark_notification_read))
        .route("/notifications/mark-all-read", post(mark_all_notifications_read))
        .route("/notifications/test", post(send_test_notification))
        .route("/notifications/{notification_id}/actions", post(execute_notification_action))
        .route("/notification-preferences", get(get_notification_preferences).put(update_notification_preferences))
        .with_state(state)
}
