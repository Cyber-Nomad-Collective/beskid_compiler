use beskid_pckg_community::CommentId;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateProfileRequest {
    pub(super) display_name: String,
    #[serde(default)]
    pub(super) bio: String,
    #[serde(default)]
    pub(super) social_links: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct CreatePostRequest {
    pub(super) title: String,
    pub(super) content: String,
}

#[derive(Deserialize)]
pub(super) struct SetBoardLockedRequest {
    pub(super) locked: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateCommentRequest {
    pub(super) content: String,
    pub(super) parent_comment_id: Option<CommentId>,
}

#[derive(Deserialize)]
pub(super) struct VoteRequest {
    pub(super) value: i8,
}

#[derive(Deserialize)]
pub(super) struct NotificationPreferenceRequest {
    #[serde(default)]
    pub(super) mode: Option<NotificationPreferenceMode>,
    #[serde(default)]
    pub(super) preferences: Option<TypedNotificationPreferenceRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TypedNotificationPreferenceRequest {
    pub(super) system_enabled: bool,
    pub(super) mention_enabled: bool,
    pub(super) reply_enabled: bool,
    pub(super) followed_publisher_post_enabled: bool,
    pub(super) moderation_enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum NotificationPreferenceMode {
    All,
    MentionsOnly,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NotificationAction {
    MarkRead,
    Dismiss,
}

#[derive(Deserialize)]
pub(super) struct NotificationActionRequest {
    pub(super) action: NotificationAction,
}
