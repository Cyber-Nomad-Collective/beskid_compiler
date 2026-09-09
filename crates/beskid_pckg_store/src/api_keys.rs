use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::package::SqlxPackageRepository;

pub const RELEASE_PUBLISHER_KEY_ID: &str = "79587ce7-3937-4e21-8f74-e6d19d056fb2";
pub const RELEASE_PUBLISHER_SUBJECT: &str = "release:github-actions";
pub const RELEASE_PUBLISHER_LABEL: &str = "GitHub Actions Release";
pub(crate) const RECONCILE_RELEASE_PUBLISHER_KEY_SQL: &str = "INSERT INTO pckg_api_keys (id,subject,label,token_sha256,scopes,created_at_utc,revoked_at_utc) \
     VALUES ($1,$2,$3,$4,$5,$6,NULL) \
     ON CONFLICT (id) DO UPDATE SET label=EXCLUDED.label,token_sha256=EXCLUDED.token_sha256,\
     scopes=EXCLUDED.scopes,revoked_at_utc=NULL \
     WHERE pckg_api_keys.subject=EXCLUDED.subject \
     RETURNING id,subject,label,scopes,created_at_utc,revoked_at_utc";

impl SqlxPackageRepository {
    /// Creates the API-key table after the package registry migration. Kept
    /// explicit so test fixtures can opt into only the surface they exercise.
    pub async fn migrate_api_keys(&self) -> Result<(), ApiKeyStoreError> {
        sqlx::raw_sql(crate::migrations::CREATE_API_KEYS).execute(self.pool()).await.map_err(api_key_database_error)?;
        Ok(())
    }

    /// Reconciles the one release-automation principal from a pre-hashed
    /// credential. The raw publisher token never enters server configuration
    /// or this persistence boundary.
    pub async fn reconcile_release_publisher_key(
        &self,
        token_sha256: &str,
        now_unix_seconds: i64,
    ) -> Result<ApiKey, ApiKeyStoreError> {
        validate_release_publisher_key_sha256(token_sha256)?;
        let id = Uuid::parse_str(RELEASE_PUBLISHER_KEY_ID).expect("release publisher key UUID is valid");
        let created_at =
            DateTime::from_timestamp(now_unix_seconds, 0).ok_or(ApiKeyStoreError::InvalidReleaseTokenHash)?;
        let scopes = vec!["read".to_owned(), "publish".to_owned()];
        let row = sqlx::query_as::<_, ApiKeyRow>(RECONCILE_RELEASE_PUBLISHER_KEY_SQL)
            .bind(id)
            .bind(RELEASE_PUBLISHER_SUBJECT)
            .bind(RELEASE_PUBLISHER_LABEL)
            .bind(token_sha256)
            .bind(&scopes)
            .bind(created_at)
            .fetch_optional(self.pool())
            .await
            .map_err(release_key_database_error)?;
        row.map(ApiKeyRow::into_domain).ok_or(ApiKeyStoreError::ReleaseKeyCollision)
    }
}

/// API-key metadata that may be shown to its owner. `token_hash` is never
/// returned through this type or any repository method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub subject: String,
    pub label: String,
    pub scopes: Vec<String>,
    pub created_at_unix_seconds: i64,
    pub revoked_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiKey {
    pub id: String,
    pub subject: String,
    pub label: String,
    pub scopes: Vec<String>,
    /// Present only for the duration of create/verify. Callers must not log or
    /// persist it outside this repository boundary.
    pub raw_token: String,
    pub now_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyStoreError {
    InvalidAuthHubSubject,
    InvalidLabel,
    InvalidScope,
    InvalidToken,
    InvalidReleaseTokenHash,
    ReleaseKeyCollision,
    Database(String),
}

/// Async, subject-scoped persistence for API-key management. All reads and
/// revocations are constrained by caller subject so key ids cannot disclose or
/// alter another owner's credentials.
#[async_trait]
pub trait AsyncApiKeyRepository: Send + Sync {
    async fn create_api_key(&self, request: NewApiKey) -> Result<ApiKey, ApiKeyStoreError>;
    async fn list_api_keys(&self, subject: &str) -> Result<Vec<ApiKey>, ApiKeyStoreError>;
    async fn revoke_api_key(&self, id: &str, subject: &str, now_unix_seconds: i64) -> Result<bool, ApiKeyStoreError>;
    async fn find_active_api_key_by_token(&self, raw_token: &str) -> Result<Option<ApiKey>, ApiKeyStoreError>;
}

#[async_trait]
impl AsyncApiKeyRepository for SqlxPackageRepository {
    async fn create_api_key(&self, request: NewApiKey) -> Result<ApiKey, ApiKeyStoreError> {
        validate_api_key_subject(&request.subject)?;
        validate_api_key_label(&request.label)?;
        validate_api_key_scopes(&request.scopes)?;
        if request.raw_token.trim().len() < 24 {
            return Err(ApiKeyStoreError::InvalidToken);
        }
        let id = Uuid::parse_str(&request.id).map_err(|_| ApiKeyStoreError::InvalidToken)?;
        let created_at = DateTime::from_timestamp(request.now_unix_seconds, 0).ok_or(ApiKeyStoreError::InvalidToken)?;
        let token_hash = api_key_token_hash(&request.raw_token);
        sqlx::query("INSERT INTO pckg_api_keys (id,subject,label,token_sha256,scopes,created_at_utc) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(id).bind(&request.subject).bind(&request.label).bind(token_hash).bind(&request.scopes).bind(created_at)
            .execute(self.pool()).await.map_err(api_key_database_error)?;
        Ok(ApiKey {
            id: request.id,
            subject: request.subject,
            label: request.label,
            scopes: request.scopes,
            created_at_unix_seconds: request.now_unix_seconds,
            revoked_at_unix_seconds: None,
        })
    }

    async fn list_api_keys(&self, subject: &str) -> Result<Vec<ApiKey>, ApiKeyStoreError> {
        validate_api_key_subject(subject)?;
        let rows = sqlx::query_as::<_, ApiKeyRow>("SELECT id,subject,label,scopes,created_at_utc,revoked_at_utc FROM pckg_api_keys WHERE subject=$1 ORDER BY created_at_utc DESC")
            .bind(subject).fetch_all(self.pool()).await.map_err(api_key_database_error)?;
        Ok(rows.into_iter().map(ApiKeyRow::into_domain).collect())
    }

    async fn revoke_api_key(&self, id: &str, subject: &str, now_unix_seconds: i64) -> Result<bool, ApiKeyStoreError> {
        validate_api_key_subject(subject)?;
        let id = Uuid::parse_str(id).map_err(|_| ApiKeyStoreError::InvalidToken)?;
        let revoked_at = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(ApiKeyStoreError::InvalidToken)?;
        let changed = sqlx::query(
            "UPDATE pckg_api_keys SET revoked_at_utc=COALESCE(revoked_at_utc,$3) WHERE id=$1 AND subject=$2",
        )
        .bind(id)
        .bind(subject)
        .bind(revoked_at)
        .execute(self.pool())
        .await
        .map_err(api_key_database_error)?;
        Ok(changed.rows_affected() > 0)
    }

    async fn find_active_api_key_by_token(&self, raw_token: &str) -> Result<Option<ApiKey>, ApiKeyStoreError> {
        if raw_token.trim().len() < 24 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, ApiKeyRow>("SELECT id,subject,label,scopes,created_at_utc,revoked_at_utc FROM pckg_api_keys WHERE token_sha256=$1 AND revoked_at_utc IS NULL")
            .bind(api_key_token_hash(raw_token)).fetch_optional(self.pool()).await.map_err(api_key_database_error)?;
        Ok(row.map(ApiKeyRow::into_domain))
    }
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    subject: String,
    label: String,
    scopes: Vec<String>,
    created_at_utc: DateTime<Utc>,
    revoked_at_utc: Option<DateTime<Utc>>,
}
impl ApiKeyRow {
    fn into_domain(self) -> ApiKey {
        ApiKey {
            id: self.id.to_string(),
            subject: self.subject,
            label: self.label,
            scopes: self.scopes,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            revoked_at_unix_seconds: self.revoked_at_utc.map(|time| time.timestamp()),
        }
    }
}

fn api_key_token_hash(raw_token: &str) -> String {
    format!("{:x}", Sha256::digest(raw_token.as_bytes()))
}

pub fn validate_release_publisher_key_sha256(token_sha256: &str) -> Result<(), ApiKeyStoreError> {
    (token_sha256.len() == 64
        && token_sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some(())
    .ok_or(ApiKeyStoreError::InvalidReleaseTokenHash)
}
fn validate_api_key_subject(subject: &str) -> Result<(), ApiKeyStoreError> {
    is_valid_api_key_subject(subject).then_some(()).ok_or(ApiKeyStoreError::InvalidAuthHubSubject)
}

fn is_valid_api_key_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    !trimmed.is_empty()
        && trimmed == subject
        && trimmed.bytes().all(|byte| byte.is_ascii_graphic())
        && trimmed.len() <= 255
}
fn validate_api_key_label(label: &str) -> Result<(), ApiKeyStoreError> {
    (!label.trim().is_empty() && label.trim() == label && label.len() <= 128)
        .then_some(())
        .ok_or(ApiKeyStoreError::InvalidLabel)
}
fn validate_api_key_scopes(scopes: &[String]) -> Result<(), ApiKeyStoreError> {
    (!scopes.is_empty() && scopes.iter().all(|scope| matches!(scope.as_str(), "read" | "publish")))
        .then_some(())
        .ok_or(ApiKeyStoreError::InvalidScope)
}
fn api_key_database_error(error: sqlx::Error) -> ApiKeyStoreError {
    ApiKeyStoreError::Database(error.to_string())
}

fn release_key_database_error(error: sqlx::Error) -> ApiKeyStoreError {
    match &error {
        sqlx::Error::Database(database) if database.code().as_deref() == Some("23505") => {
            ApiKeyStoreError::ReleaseKeyCollision
        }
        _ => api_key_database_error(error),
    }
}
