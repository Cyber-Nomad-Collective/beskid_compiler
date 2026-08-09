use chrono::{DateTime, Utc};
use uuid::Uuid;

pub(super) fn parse_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value).ok()
}

pub(super) fn nonnegative_u64(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

pub(super) fn utc_timestamp(value: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(value, 0)
}

pub(super) fn is_unique_violation(error: &sqlx::Error) -> bool {
    error.as_database_error().and_then(|database| database.code()).is_some_and(|code| code == "23505")
}

pub(super) fn database_message(error: sqlx::Error) -> String {
    error.to_string()
}

