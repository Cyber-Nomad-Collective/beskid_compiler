use chrono::{DateTime, Utc};

use super::model::CommunityStoreError;

pub(super) fn validate_community_subject(subject: &str) -> Result<(), CommunityStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(CommunityStoreError::InvalidAuthHubSubject)
}
pub(super) fn validate_nonblank(value: &str) -> Result<(), CommunityStoreError> {
    (!value.trim().is_empty() && value == value.trim()).then_some(()).ok_or(CommunityStoreError::InvalidContent)
}
pub(super) fn validate_notification_scope(scope: &str) -> Result<(), CommunityStoreError> {
    matches!(scope, "system" | "mention" | "reply" | "followed_publisher_post" | "moderation")
        .then_some(())
        .ok_or(CommunityStoreError::InvalidContent)
}
pub(super) fn community_timestamp(value: i64) -> Result<DateTime<Utc>, CommunityStoreError> {
    DateTime::from_timestamp(value, 0).ok_or(CommunityStoreError::InvalidContent)
}
pub(super) fn community_database_error(error: sqlx::Error) -> CommunityStoreError {
    CommunityStoreError::Database(error.to_string())
}
