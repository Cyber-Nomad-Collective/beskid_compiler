use async_trait::async_trait;
use beskid_pckg_operations::BlockedLinkPattern;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::package::SqlxPackageRepository;

impl SqlxPackageRepository {
    /// Applies the independent operations tables.  This stays separate from
    /// role management so focused fixtures can opt into the exact durable
    /// boundary they exercise.
    pub async fn migrate_registry_operations(&self) -> Result<(), RegistryOperationsStoreError> {
        sqlx::raw_sql(crate::migrations::CREATE_REGISTRY_OPERATIONS)
            .execute(self.pool())
            .await
            .map_err(registry_operations_database_error)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedLinkPolicy {
    pub id: String,
    pub pattern: String,
    pub note: Option<String>,
    pub created_by_subject: String,
    pub created_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBlockedLinkPolicy {
    pub id: String,
    pub pattern: String,
    pub note: Option<String>,
    pub created_by_subject: String,
    pub created_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryActivity {
    pub sequence: i64,
    pub occurred_at_unix_seconds: i64,
    pub severity: String,
    pub action: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub actor_subject: Option<String>,
    pub package_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRegistryActivity {
    pub occurred_at_unix_seconds: i64,
    pub severity: String,
    pub action: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub actor_subject: Option<String>,
    pub package_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklySpotlightRun {
    pub id: String,
    pub ran_by_subject: String,
    pub ran_at_unix_seconds: i64,
    pub activity_count: u64,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryOperationsStoreError {
    InvalidAuthHubSubject,
    InvalidBlockedLinkPattern,
    InvalidBlockedLinkId,
    DuplicateBlockedLinkPattern,
    InvalidActivity,
    InvalidWeeklySpotlightRun,
    NotFound,
    Database(String),
}

#[async_trait]
pub trait AsyncRegistryOperationsRepository: Send + Sync {
    async fn list_blocked_link_policies(&self) -> Result<Vec<BlockedLinkPolicy>, RegistryOperationsStoreError>;
    async fn add_blocked_link_policy(
        &self,
        policy: NewBlockedLinkPolicy,
    ) -> Result<BlockedLinkPolicy, RegistryOperationsStoreError>;
    async fn delete_blocked_link_policy(&self, id: &str) -> Result<(), RegistryOperationsStoreError>;
    async fn append_registry_activity(
        &self,
        activity: NewRegistryActivity,
    ) -> Result<RegistryActivity, RegistryOperationsStoreError>;
    async fn recent_registry_activity(&self, take: u16) -> Result<Vec<RegistryActivity>, RegistryOperationsStoreError>;
    async fn record_weekly_spotlight(
        &self,
        run: WeeklySpotlightRun,
    ) -> Result<WeeklySpotlightRun, RegistryOperationsStoreError>;
}

#[async_trait]
impl AsyncRegistryOperationsRepository for SqlxPackageRepository {
    async fn list_blocked_link_policies(&self) -> Result<Vec<BlockedLinkPolicy>, RegistryOperationsStoreError> {
        let rows = sqlx::query_as::<_, BlockedLinkPolicyRow>(
            "SELECT id,pattern,note,created_by_subject,created_at_utc \
             FROM pckg_blocked_link_patterns ORDER BY created_at_utc DESC,id DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(registry_operations_database_error)?;
        Ok(rows.into_iter().map(BlockedLinkPolicyRow::into_domain).collect())
    }

    async fn add_blocked_link_policy(
        &self,
        policy: NewBlockedLinkPolicy,
    ) -> Result<BlockedLinkPolicy, RegistryOperationsStoreError> {
        validate_registry_operations_subject(&policy.created_by_subject)?;
        let pattern = BlockedLinkPattern::new(&policy.pattern)
            .map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkPattern)?;
        let id = Uuid::parse_str(&policy.id).map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkId)?;
        let at = registry_operations_timestamp(policy.created_at_unix_seconds)?;
        let note = normalize_operations_note(policy.note)?;
        let row = sqlx::query_as::<_, BlockedLinkPolicyRow>(
            "INSERT INTO pckg_blocked_link_patterns \
             (id,pattern,note,created_by_subject,created_at_utc) VALUES ($1,$2,$3,$4,$5) \
             RETURNING id,pattern,note,created_by_subject,created_at_utc",
        )
        .bind(id)
        .bind(pattern.as_str())
        .bind(note)
        .bind(policy.created_by_subject)
        .bind(at)
        .fetch_one(self.pool())
        .await
        .map_err(registry_operations_insert_error)?;
        Ok(row.into_domain())
    }

    async fn delete_blocked_link_policy(&self, id: &str) -> Result<(), RegistryOperationsStoreError> {
        let id = Uuid::parse_str(id).map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkId)?;
        let deleted = sqlx::query("DELETE FROM pckg_blocked_link_patterns WHERE id=$1")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(registry_operations_database_error)?;
        if deleted.rows_affected() == 0 {
            return Err(RegistryOperationsStoreError::NotFound);
        }
        Ok(())
    }

    async fn append_registry_activity(
        &self,
        activity: NewRegistryActivity,
    ) -> Result<RegistryActivity, RegistryOperationsStoreError> {
        validate_registry_activity(&activity)?;
        let at = registry_operations_timestamp(activity.occurred_at_unix_seconds)?;
        let mut transaction = self.pool().begin().await.map_err(registry_operations_database_error)?;
        let row = sqlx::query_as::<_, RegistryActivityRow>(
            "INSERT INTO pckg_registry_activity \
             (occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
             RETURNING id,occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version",
        )
        .bind(at)
        .bind(activity.severity)
        .bind(activity.action)
        .bind(activity.message)
        .bind(activity.trace_id)
        .bind(activity.actor_subject)
        .bind(activity.package_name)
        .bind(activity.version)
        .fetch_one(&mut *transaction)
        .await
        .map_err(registry_operations_database_error)?;
        // Retain the legacy 500-entry diagnostic window atomically with the append.
        sqlx::query(
            "DELETE FROM pckg_registry_activity WHERE id IN ( \
             SELECT id FROM pckg_registry_activity ORDER BY occurred_at_utc DESC,id DESC OFFSET 500)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(registry_operations_database_error)?;
        transaction.commit().await.map_err(registry_operations_database_error)?;
        Ok(row.into_domain())
    }

    async fn recent_registry_activity(&self, take: u16) -> Result<Vec<RegistryActivity>, RegistryOperationsStoreError> {
        let take = i64::from(take.clamp(1, 500));
        let rows = sqlx::query_as::<_, RegistryActivityRow>(
            "SELECT id,occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version \
             FROM pckg_registry_activity ORDER BY occurred_at_utc DESC,id DESC LIMIT $1",
        )
        .bind(take)
        .fetch_all(self.pool())
        .await
        .map_err(registry_operations_database_error)?;
        Ok(rows.into_iter().map(RegistryActivityRow::into_domain).collect())
    }

    async fn record_weekly_spotlight(
        &self,
        run: WeeklySpotlightRun,
    ) -> Result<WeeklySpotlightRun, RegistryOperationsStoreError> {
        validate_registry_operations_subject(&run.ran_by_subject)?;
        if run.delivery != "in_app_only" {
            return Err(RegistryOperationsStoreError::InvalidWeeklySpotlightRun);
        }
        let id = Uuid::parse_str(&run.id).map_err(|_| RegistryOperationsStoreError::InvalidWeeklySpotlightRun)?;
        let at = registry_operations_timestamp(run.ran_at_unix_seconds)?;
        let activity_count =
            i64::try_from(run.activity_count).map_err(|_| RegistryOperationsStoreError::InvalidWeeklySpotlightRun)?;
        sqlx::query("INSERT INTO pckg_weekly_spotlight_runs (id,ran_by_subject,ran_at_utc,activity_count,delivery) VALUES ($1,$2,$3,$4,$5)")
            .bind(id).bind(&run.ran_by_subject).bind(at).bind(activity_count).bind(&run.delivery)
            .execute(self.pool()).await.map_err(registry_operations_database_error)?;
        Ok(run)
    }
}

#[derive(sqlx::FromRow)]
struct BlockedLinkPolicyRow {
    id: Uuid,
    pattern: String,
    note: Option<String>,
    created_by_subject: String,
    created_at_utc: DateTime<Utc>,
}

impl BlockedLinkPolicyRow {
    fn into_domain(self) -> BlockedLinkPolicy {
        BlockedLinkPolicy {
            id: self.id.to_string(),
            pattern: self.pattern,
            note: self.note,
            created_by_subject: self.created_by_subject,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct RegistryActivityRow {
    id: i64,
    occurred_at_utc: DateTime<Utc>,
    severity: String,
    action: String,
    message: String,
    trace_id: Option<String>,
    actor_subject: Option<String>,
    package_name: Option<String>,
    version: Option<String>,
}

impl RegistryActivityRow {
    fn into_domain(self) -> RegistryActivity {
        RegistryActivity {
            sequence: self.id,
            occurred_at_unix_seconds: self.occurred_at_utc.timestamp(),
            severity: self.severity,
            action: self.action,
            message: self.message,
            trace_id: self.trace_id,
            actor_subject: self.actor_subject,
            package_name: self.package_name,
            version: self.version,
        }
    }
}

fn validate_registry_operations_subject(subject: &str) -> Result<(), RegistryOperationsStoreError> {
    is_valid_registry_operations_subject(subject)
        .then_some(())
        .ok_or(RegistryOperationsStoreError::InvalidAuthHubSubject)
}

fn is_valid_registry_operations_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    !trimmed.is_empty()
        && trimmed == subject
        && trimmed.bytes().all(|byte| byte.is_ascii_graphic())
        && trimmed.len() <= 255
}

fn registry_operations_timestamp(value: i64) -> Result<DateTime<Utc>, RegistryOperationsStoreError> {
    DateTime::from_timestamp(value, 0).ok_or(RegistryOperationsStoreError::InvalidActivity)
}

fn normalize_operations_note(note: Option<String>) -> Result<Option<String>, RegistryOperationsStoreError> {
    let note = note.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()));
    if note.as_ref().is_some_and(|value| value.len() > 2000) {
        return Err(RegistryOperationsStoreError::InvalidBlockedLinkPattern);
    }
    Ok(note)
}

fn validate_registry_activity(activity: &NewRegistryActivity) -> Result<(), RegistryOperationsStoreError> {
    if activity.severity.trim().is_empty()
        || activity.severity.len() > 64
        || activity.action.trim().is_empty()
        || activity.action.len() > 128
        || activity.message.len() > 4000
        || activity.trace_id.as_ref().is_some_and(|value| value.len() > 256)
        || activity.package_name.as_ref().is_some_and(|value| value.len() > 256)
        || activity.version.as_ref().is_some_and(|value| value.len() > 128)
    {
        return Err(RegistryOperationsStoreError::InvalidActivity);
    }
    if let Some(subject) = activity.actor_subject.as_deref() {
        validate_registry_operations_subject(subject)?;
    }
    Ok(())
}

fn registry_operations_database_error(error: sqlx::Error) -> RegistryOperationsStoreError {
    RegistryOperationsStoreError::Database(error.to_string())
}

fn registry_operations_insert_error(error: sqlx::Error) -> RegistryOperationsStoreError {
    if error.as_database_error().and_then(|database| database.code()).is_some_and(|code| code == "23505") {
        RegistryOperationsStoreError::DuplicateBlockedLinkPattern
    } else {
        registry_operations_database_error(error)
    }
}
