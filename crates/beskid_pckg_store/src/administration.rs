use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::package::{validate_subject, SqlxPackageRepository};

impl SqlxPackageRepository {
    /// Applies registry-owned administration tables. It deliberately creates
    /// no role rows, so a deployment cannot accidentally bootstrap privilege.
    pub async fn migrate_administration(&self) -> Result<(), AdministrationStoreError> {
        sqlx::raw_sql(crate::migrations::CREATE_ADMINISTRATION)
            .execute(self.pool())
            .await
            .map_err(administration_database_error)?;
        Ok(())
    }

    /// Seeds exactly one initial SuperAdmin only when the complete role table
    /// is empty. The caller must supply an explicit `github:<numeric-id>` from
    /// deployment configuration; a later config change cannot elevate another
    /// subject once any administrator has been recorded.
    pub async fn bootstrap_super_admin(
        &self,
        subject: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        let timestamp = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(AdministrationStoreError::InvalidRole)?;
        let inserted = sqlx::query("INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) SELECT $1,'superadmin',$1,$2 WHERE NOT EXISTS (SELECT 1 FROM pckg_admin_roles)")
            .bind(subject).bind(timestamp).execute(self.pool()).await.map_err(administration_database_error)?;
        Ok(inserted.rows_affected() == 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminRole {
    User,
    Moderator,
    SuperAdmin,
}

impl AdminRole {
    fn stored(self) -> Option<&'static str> {
        match self {
            Self::User => None,
            Self::Moderator => Some("moderator"),
            Self::SuperAdmin => Some("superadmin"),
        }
    }
    fn from_stored(value: &str) -> Option<Self> {
        match value {
            "moderator" => Some(Self::Moderator),
            "superadmin" => Some(Self::SuperAdmin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRoleAssignment {
    pub subject: String,
    pub role: AdminRole,
    pub granted_by_subject: String,
    pub granted_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublisherVerification {
    pub subject: String,
    pub is_verified: bool,
    pub reviewed_by_subject: String,
    pub reviewed_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePermissionGrant {
    pub subject: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub capability: String,
    pub granted_by_subject: String,
    pub granted_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReviewDecision {
    pub package_id: String,
    pub version: Option<String>,
    pub decision: String,
    pub reason: String,
    pub decided_by_subject: String,
    pub decided_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdministrationStoreError {
    InvalidAuthHubSubject,
    InvalidRole,
    InvalidResource,
    InvalidDecision,
    InvalidPackageId,
    Database(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReviewRequest {
    pub id: String,
    pub package_id: String,
    pub requested_by_subject: String,
    pub reason: String,
    pub status: String,
    pub submitted_at_unix_seconds: i64,
    pub reviewer_subject: Option<String>,
    pub review_notes: Option<String>,
    pub reviewed_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageReviewQueueError {
    InvalidAuthHubSubject,
    InvalidPackageId,
    InvalidReviewId,
    InvalidReason,
    InvalidAction,
    NotFound,
    Database(String),
}

#[async_trait]
pub trait AsyncPackageReviewRepository: Send + Sync {
    async fn submit_package_review(
        &self,
        review: PackageReviewRequest,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError>;
    async fn list_package_reviews(&self) -> Result<Vec<PackageReviewRequest>, PackageReviewQueueError>;
    async fn action_package_review(
        &self,
        review_id: &str,
        action: &str,
        reviewer_subject: &str,
        notes: Option<String>,
        reviewed_at_unix_seconds: i64,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError>;
}

#[async_trait]
pub trait AsyncAdministrationRepository: Send + Sync {
    async fn list_admin_roles(&self) -> Result<Vec<AdminRoleAssignment>, AdministrationStoreError>;
    async fn roles_for_subject(&self, subject: &str) -> Result<Vec<AdminRole>, AdministrationStoreError>;
    async fn grant_admin_role(&self, assignment: AdminRoleAssignment) -> Result<(), AdministrationStoreError>;
    async fn replace_admin_roles(
        &self,
        subject: &str,
        roles: Vec<AdminRole>,
        granted_by_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), AdministrationStoreError>;
    async fn set_publisher_verification(
        &self,
        verification: PublisherVerification,
    ) -> Result<(), AdministrationStoreError>;
    async fn get_publisher_verification(
        &self,
        subject: &str,
    ) -> Result<Option<PublisherVerification>, AdministrationStoreError>;
    async fn list_publisher_verifications(&self) -> Result<Vec<PublisherVerification>, AdministrationStoreError>;
    async fn grant_resource_permission(&self, grant: ResourcePermissionGrant) -> Result<(), AdministrationStoreError>;
    async fn list_resource_permissions(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError>;
    async fn list_all_resource_permissions(&self) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError>;
    async fn record_package_review(&self, decision: PackageReviewDecision) -> Result<(), AdministrationStoreError>;
}

#[async_trait]
impl AsyncAdministrationRepository for SqlxPackageRepository {
    async fn list_admin_roles(&self) -> Result<Vec<AdminRoleAssignment>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, AdminRoleRow>(
            "SELECT subject, role, granted_by_subject, granted_at_utc FROM pckg_admin_roles ORDER BY subject, role",
        )
        .fetch_all(self.pool())
        .await
        .map_err(administration_database_error)?;
        rows.into_iter().map(AdminRoleRow::into_domain).collect()
    }

    async fn roles_for_subject(&self, subject: &str) -> Result<Vec<AdminRole>, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        let roles = sqlx::query_scalar::<_, String>("SELECT role FROM pckg_admin_roles WHERE subject=$1")
            .bind(subject)
            .fetch_all(self.pool())
            .await
            .map_err(administration_database_error)?;
        Ok(roles.into_iter().filter_map(|role| AdminRole::from_stored(&role)).collect())
    }

    async fn grant_admin_role(&self, assignment: AdminRoleAssignment) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&assignment.subject)?;
        validate_administration_subject(&assignment.granted_by_subject)?;
        let Some(role) = assignment.role.stored() else {
            return Err(AdministrationStoreError::InvalidRole);
        };
        let timestamp = DateTime::from_timestamp(assignment.granted_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidRole)?;
        sqlx::query("INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT (subject,role) DO NOTHING")
            .bind(assignment.subject).bind(role).bind(assignment.granted_by_subject).bind(timestamp)
            .execute(self.pool()).await.map_err(administration_database_error)?;
        Ok(())
    }
    async fn replace_admin_roles(
        &self,
        subject: &str,
        roles: Vec<AdminRole>,
        granted_by_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(subject)?;
        validate_administration_subject(granted_by_subject)?;
        if roles.iter().any(|role| role.stored().is_none()) {
            return Err(AdministrationStoreError::InvalidRole);
        }
        let timestamp = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(AdministrationStoreError::InvalidRole)?;
        let mut transaction = self.pool().begin().await.map_err(administration_database_error)?;
        sqlx::query("DELETE FROM pckg_admin_roles WHERE subject=$1")
            .bind(subject)
            .execute(&mut *transaction)
            .await
            .map_err(administration_database_error)?;
        for role in roles {
            sqlx::query(
                "INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4)",
            )
            .bind(subject)
            .bind(role.stored().expect("validated role"))
            .bind(granted_by_subject)
            .bind(timestamp)
            .execute(&mut *transaction)
            .await
            .map_err(administration_database_error)?;
        }
        transaction.commit().await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn set_publisher_verification(
        &self,
        verification: PublisherVerification,
    ) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&verification.subject)?;
        validate_administration_subject(&verification.reviewed_by_subject)?;
        let timestamp = DateTime::from_timestamp(verification.reviewed_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidDecision)?;
        sqlx::query("INSERT INTO pckg_publisher_verifications (subject,is_verified,reviewed_by_subject,reviewed_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT (subject) DO UPDATE SET is_verified=EXCLUDED.is_verified, reviewed_by_subject=EXCLUDED.reviewed_by_subject, reviewed_at_utc=EXCLUDED.reviewed_at_utc")
            .bind(verification.subject).bind(verification.is_verified).bind(verification.reviewed_by_subject).bind(timestamp)
            .execute(self.pool()).await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn get_publisher_verification(
        &self,
        subject: &str,
    ) -> Result<Option<PublisherVerification>, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        sqlx::query_as::<_, PublisherVerificationRow>("SELECT subject,is_verified,reviewed_by_subject,reviewed_at_utc FROM pckg_publisher_verifications WHERE subject=$1")
            .bind(subject).fetch_optional(self.pool()).await.map_err(administration_database_error)?
            .map(PublisherVerificationRow::into_domain).transpose()
    }
    async fn list_publisher_verifications(&self) -> Result<Vec<PublisherVerification>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, PublisherVerificationRow>("SELECT subject,is_verified,reviewed_by_subject,reviewed_at_utc FROM pckg_publisher_verifications ORDER BY subject").fetch_all(self.pool()).await.map_err(administration_database_error)?;
        rows.into_iter().map(PublisherVerificationRow::into_domain).collect()
    }

    async fn grant_resource_permission(&self, grant: ResourcePermissionGrant) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&grant.subject)?;
        validate_administration_subject(&grant.granted_by_subject)?;
        validate_resource(&grant.resource_kind, &grant.resource_id, &grant.capability)?;
        let timestamp = DateTime::from_timestamp(grant.granted_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidResource)?;
        sqlx::query("INSERT INTO pckg_resource_permissions (subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (subject,resource_kind,resource_id,capability) DO NOTHING")
            .bind(grant.subject).bind(grant.resource_kind).bind(grant.resource_id).bind(grant.capability).bind(grant.granted_by_subject).bind(timestamp)
            .execute(self.pool()).await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn list_resource_permissions(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError> {
        validate_resource(resource_kind, resource_id, "moderate")?;
        let rows = sqlx::query_as::<_, ResourcePermissionRow>("SELECT subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc FROM pckg_resource_permissions WHERE resource_kind=$1 AND resource_id=$2 ORDER BY subject")
            .bind(resource_kind).bind(resource_id).fetch_all(self.pool()).await.map_err(administration_database_error)?;
        rows.into_iter().map(ResourcePermissionRow::into_domain).collect()
    }

    async fn list_all_resource_permissions(&self) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, ResourcePermissionRow>("SELECT subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc FROM pckg_resource_permissions ORDER BY resource_kind,resource_id,subject")
            .fetch_all(self.pool()).await.map_err(administration_database_error)?;
        rows.into_iter().map(ResourcePermissionRow::into_domain).collect()
    }

    async fn record_package_review(&self, decision: PackageReviewDecision) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&decision.decided_by_subject)?;
        if !matches!(decision.decision.as_str(), "approved" | "rejected" | "yanked" | "unyanked")
            || decision.reason.len() > 4000
        {
            return Err(AdministrationStoreError::InvalidDecision);
        }
        let package_id =
            Uuid::parse_str(&decision.package_id).map_err(|_| AdministrationStoreError::InvalidPackageId)?;
        let timestamp = DateTime::from_timestamp(decision.decided_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidDecision)?;
        sqlx::query("INSERT INTO pckg_package_review_decisions (package_id,version,decision,reason,decided_by_subject,decided_at_utc) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(package_id).bind(decision.version).bind(decision.decision).bind(decision.reason).bind(decision.decided_by_subject).bind(timestamp)
            .execute(self.pool()).await.map_err(administration_database_error)?;
        Ok(())
    }
}

#[async_trait]
impl AsyncPackageReviewRepository for SqlxPackageRepository {
    async fn submit_package_review(
        &self,
        review: PackageReviewRequest,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        validate_review_request(&review)?;
        let id = Uuid::parse_str(&review.id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
        let package_id = Uuid::parse_str(&review.package_id).map_err(|_| PackageReviewQueueError::InvalidPackageId)?;
        let submitted_at = DateTime::from_timestamp(review.submitted_at_unix_seconds, 0)
            .ok_or(PackageReviewQueueError::InvalidReason)?;
        let row = sqlx::query_as::<_, PackageReviewRequestRow>(
            "INSERT INTO pckg_package_review_requests \
             (id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc) \
             VALUES ($1,$2,$3,$4,'pending',$5,NULL,NULL,NULL) \
             RETURNING id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc",
        )
        .bind(id)
        .bind(package_id)
        .bind(&review.requested_by_subject)
        .bind(&review.reason)
        .bind(submitted_at)
        .fetch_one(self.pool())
        .await
        .map_err(review_queue_database_error)?;
        row.into_domain()
    }

    async fn list_package_reviews(&self) -> Result<Vec<PackageReviewRequest>, PackageReviewQueueError> {
        let rows = sqlx::query_as::<_, PackageReviewRequestRow>(
            "SELECT id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc \
             FROM pckg_package_review_requests ORDER BY submitted_at_utc DESC, id DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(review_queue_database_error)?;
        rows.into_iter().map(PackageReviewRequestRow::into_domain).collect()
    }

    async fn action_package_review(
        &self,
        review_id: &str,
        action: &str,
        reviewer_subject: &str,
        notes: Option<String>,
        reviewed_at_unix_seconds: i64,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        let id = Uuid::parse_str(review_id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
        validate_subject_for_review(reviewer_subject)?;
        let action = normalize_review_action(action)?;
        let notes = normalize_review_notes(notes)?;
        let reviewed_at =
            DateTime::from_timestamp(reviewed_at_unix_seconds, 0).ok_or(PackageReviewQueueError::InvalidAction)?;
        let row = sqlx::query_as::<_, PackageReviewRequestRow>(
            "UPDATE pckg_package_review_requests \
             SET status=$2, reviewer_subject=$3, review_notes=$4, reviewed_at_utc=$5 \
             WHERE id=$1 \
             RETURNING id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc",
        )
        .bind(id)
        .bind(action)
        .bind(reviewer_subject)
        .bind(notes)
        .bind(reviewed_at)
        .fetch_optional(self.pool())
        .await
        .map_err(review_queue_database_error)?
        .ok_or(PackageReviewQueueError::NotFound)?;
        row.into_domain()
    }
}

#[derive(sqlx::FromRow)]
struct PackageReviewRequestRow {
    id: Uuid,
    package_id: Uuid,
    requested_by_subject: String,
    reason: String,
    status: String,
    submitted_at_utc: DateTime<Utc>,
    reviewer_subject: Option<String>,
    review_notes: Option<String>,
    reviewed_at_utc: Option<DateTime<Utc>>,
}

impl PackageReviewRequestRow {
    fn into_domain(self) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        validate_subject_for_review(&self.requested_by_subject)?;
        if let Some(subject) = &self.reviewer_subject {
            validate_subject_for_review(subject)?;
        }
        normalize_review_action(&self.status)?;
        Ok(PackageReviewRequest {
            id: self.id.to_string(),
            package_id: self.package_id.to_string(),
            requested_by_subject: self.requested_by_subject,
            reason: self.reason,
            status: self.status,
            submitted_at_unix_seconds: self.submitted_at_utc.timestamp(),
            reviewer_subject: self.reviewer_subject,
            review_notes: self.review_notes,
            reviewed_at_unix_seconds: self.reviewed_at_utc.map(|time| time.timestamp()),
        })
    }
}

fn validate_review_request(review: &PackageReviewRequest) -> Result<(), PackageReviewQueueError> {
    Uuid::parse_str(&review.id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
    Uuid::parse_str(&review.package_id).map_err(|_| PackageReviewQueueError::InvalidPackageId)?;
    validate_subject_for_review(&review.requested_by_subject)?;
    if review.reason.trim().is_empty() || review.reason.trim() != review.reason || review.reason.len() > 4000 {
        return Err(PackageReviewQueueError::InvalidReason);
    }
    Ok(())
}

fn validate_subject_for_review(subject: &str) -> Result<(), PackageReviewQueueError> {
    validate_subject(subject).map_err(|_| PackageReviewQueueError::InvalidAuthHubSubject)
}

fn normalize_review_action(action: &str) -> Result<&'static str, PackageReviewQueueError> {
    let normalized = action.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "approved" => Ok("approved"),
        "needs_changes" | "needschanges" => Ok("needs_changes"),
        "rejected" => Ok("rejected"),
        _ => Err(PackageReviewQueueError::InvalidAction),
    }
}

fn normalize_review_notes(notes: Option<String>) -> Result<Option<String>, PackageReviewQueueError> {
    let notes = notes.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()));
    if notes.as_ref().is_some_and(|value| value.len() > 4000) {
        return Err(PackageReviewQueueError::InvalidAction);
    }
    Ok(notes)
}

fn review_queue_database_error(error: sqlx::Error) -> PackageReviewQueueError {
    PackageReviewQueueError::Database(error.to_string())
}

#[derive(sqlx::FromRow)]
struct AdminRoleRow {
    subject: String,
    role: String,
    granted_by_subject: String,
    granted_at_utc: DateTime<Utc>,
}
impl AdminRoleRow {
    fn into_domain(self) -> Result<AdminRoleAssignment, AdministrationStoreError> {
        Ok(AdminRoleAssignment {
            subject: self.subject,
            role: AdminRole::from_stored(&self.role).ok_or(AdministrationStoreError::InvalidRole)?,
            granted_by_subject: self.granted_by_subject,
            granted_at_unix_seconds: self.granted_at_utc.timestamp(),
        })
    }
}
#[derive(sqlx::FromRow)]
struct PublisherVerificationRow {
    subject: String,
    is_verified: bool,
    reviewed_by_subject: String,
    reviewed_at_utc: DateTime<Utc>,
}
impl PublisherVerificationRow {
    fn into_domain(self) -> Result<PublisherVerification, AdministrationStoreError> {
        Ok(PublisherVerification {
            subject: self.subject,
            is_verified: self.is_verified,
            reviewed_by_subject: self.reviewed_by_subject,
            reviewed_at_unix_seconds: self.reviewed_at_utc.timestamp(),
        })
    }
}
#[derive(sqlx::FromRow)]
struct ResourcePermissionRow {
    subject: String,
    resource_kind: String,
    resource_id: String,
    capability: String,
    granted_by_subject: String,
    granted_at_utc: DateTime<Utc>,
}
impl ResourcePermissionRow {
    fn into_domain(self) -> Result<ResourcePermissionGrant, AdministrationStoreError> {
        validate_resource(&self.resource_kind, &self.resource_id, &self.capability)?;
        Ok(ResourcePermissionGrant {
            subject: self.subject,
            resource_kind: self.resource_kind,
            resource_id: self.resource_id,
            capability: self.capability,
            granted_by_subject: self.granted_by_subject,
            granted_at_unix_seconds: self.granted_at_utc.timestamp(),
        })
    }
}
fn validate_administration_subject(subject: &str) -> Result<(), AdministrationStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(AdministrationStoreError::InvalidAuthHubSubject)
}
fn validate_resource(kind: &str, id: &str, capability: &str) -> Result<(), AdministrationStoreError> {
    (matches!(kind, "package" | "board") && !id.trim().is_empty() && id.trim() == id && capability == "moderate")
        .then_some(())
        .ok_or(AdministrationStoreError::InvalidResource)
}
fn administration_database_error(error: sqlx::Error) -> AdministrationStoreError {
    AdministrationStoreError::Database(error.to_string())
}
