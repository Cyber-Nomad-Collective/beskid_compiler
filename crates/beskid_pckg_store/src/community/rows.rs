use chrono::{DateTime, Utc};

use super::model::{
    CommunityBoard, CommunityComment, CommunityNotification, CommunityNotificationPreference, CommunityPost,
    CommunityProfile,
};

#[derive(sqlx::FromRow)]
pub(super) struct CommunityPostRow {
    id: i64,
    board_id: String,
    author_subject: String,
    title: String,
    content: String,
    score: i32,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(super) struct CommunityProfileRow {
    subject: String,
    display_name: String,
    bio: String,
    social_links_json: String,
    is_publisher_verified: bool,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityProfileRow {
    pub(super) fn into_domain(self) -> CommunityProfile {
        CommunityProfile {
            subject: self.subject,
            display_name: self.display_name,
            bio: self.bio,
            social_links_json: self.social_links_json,
            is_publisher_verified: self.is_publisher_verified,
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct CommunityBoardRow {
    id: String,
    title: String,
    locked: bool,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityBoardRow {
    pub(super) fn into_domain(self) -> CommunityBoard {
        CommunityBoard {
            id: self.id,
            title: self.title,
            locked: self.locked,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct CommunityPreferenceRow {
    system_enabled: bool,
    mention_enabled: bool,
    reply_enabled: bool,
    followed_publisher_post_enabled: bool,
    moderation_enabled: bool,
}
impl CommunityPreferenceRow {
    pub(super) fn into_domain(self) -> CommunityNotificationPreference {
        CommunityNotificationPreference {
            system_enabled: self.system_enabled,
            mention_enabled: self.mention_enabled,
            reply_enabled: self.reply_enabled,
            followed_publisher_post_enabled: self.followed_publisher_post_enabled,
            moderation_enabled: self.moderation_enabled,
        }
    }
}
impl CommunityPostRow {
    pub(super) fn into_domain(self) -> CommunityPost {
        CommunityPost {
            id: self.id,
            board_id: self.board_id,
            author_subject: self.author_subject,
            title: self.title,
            content: self.content,
            score: self.score,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct CommunityCommentRow {
    id: i64,
    post_id: i64,
    author_subject: String,
    content: String,
    parent_comment_id: Option<i64>,
    score: i32,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityCommentRow {
    pub(super) fn into_domain(self) -> CommunityComment {
        CommunityComment {
            id: self.id,
            post_id: self.post_id,
            author_subject: self.author_subject,
            content: self.content,
            parent_comment_id: self.parent_comment_id,
            score: self.score,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct CommunityNotificationRow {
    id: i64,
    recipient_subject: String,
    scope: String,
    actor_subject: String,
    post_id: Option<i64>,
    comment_id: Option<i64>,
    created_at_utc: DateTime<Utc>,
    read_at_utc: Option<DateTime<Utc>>,
}
impl CommunityNotificationRow {
    pub(super) fn into_domain(self) -> CommunityNotification {
        CommunityNotification {
            id: self.id,
            recipient_subject: self.recipient_subject,
            scope: self.scope,
            actor_subject: self.actor_subject,
            post_id: self.post_id,
            comment_id: self.comment_id,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            read_at_unix_seconds: self.read_at_utc.map(|v| v.timestamp()),
        }
    }
}
