use async_trait::async_trait;
use chrono::DateTime;
use uuid::Uuid;

use crate::package::{SqlxPackageRepository, validate_subject};

/// A package-scoped review (rating + comment) authored by an authenticated
/// subject. These are simple package ratings served from the registry; the
/// forum-style community surface is handled by NodeBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommunityReview {
    pub id: String,
    pub package_id: String,
    pub author_subject: String,
    pub rating: i16,
    pub comment: String,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageCommunityReviewError {
    InvalidAuthHubSubject,
    InvalidPackageId,
    InvalidRating,
    InvalidComment,
    Database(String),
}

#[async_trait]
pub trait AsyncPackageCommunityReviewRepository: Send + Sync {
    async fn upsert_package_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError>;
    async fn list_package_community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError>;
}

#[async_trait]
impl AsyncPackageCommunityReviewRepository for SqlxPackageRepository {
    async fn upsert_package_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError> {
        let id = Uuid::parse_str(&review.id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        let package_id =
            Uuid::parse_str(&review.package_id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        validate_subject(&review.author_subject).map_err(|_| PackageCommunityReviewError::InvalidAuthHubSubject)?;
        if !(1..=5).contains(&review.rating) {
            return Err(PackageCommunityReviewError::InvalidRating);
        }
        if review.comment.trim().is_empty() {
            return Err(PackageCommunityReviewError::InvalidComment);
        }
        let created = DateTime::from_timestamp(review.created_at_unix_seconds, 0)
            .ok_or(PackageCommunityReviewError::InvalidComment)?;
        let updated = DateTime::from_timestamp(review.updated_at_unix_seconds, 0)
            .ok_or(PackageCommunityReviewError::InvalidComment)?;
        let row = sqlx::query_as::<_, PackageCommunityReviewRow>(
            "INSERT INTO pckg_package_community_reviews (id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (package_id,author_subject) DO UPDATE SET rating=EXCLUDED.rating,comment=EXCLUDED.comment,updated_at_utc=EXCLUDED.updated_at_utc RETURNING id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc"
        ).bind(id).bind(package_id).bind(&review.author_subject).bind(review.rating).bind(&review.comment).bind(created).bind(updated).fetch_one(self.pool()).await.map_err(|error| PackageCommunityReviewError::Database(error.to_string()))?;
        row.into_domain()
    }
    async fn list_package_community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError> {
        let package_id = Uuid::parse_str(package_id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        let rows = sqlx::query_as::<_, PackageCommunityReviewRow>("SELECT id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc FROM pckg_package_community_reviews WHERE package_id=$1 ORDER BY created_at_utc DESC,id DESC").bind(package_id).fetch_all(self.pool()).await.map_err(|error| PackageCommunityReviewError::Database(error.to_string()))?;
        rows.into_iter().map(PackageCommunityReviewRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct PackageCommunityReviewRow {
    id: Uuid,
    package_id: Uuid,
    author_subject: String,
    rating: i16,
    comment: String,
    created_at_utc: chrono::DateTime<chrono::Utc>,
    updated_at_utc: chrono::DateTime<chrono::Utc>,
}
impl PackageCommunityReviewRow {
    fn into_domain(self) -> Result<PackageCommunityReview, PackageCommunityReviewError> {
        Ok(PackageCommunityReview {
            id: self.id.to_string(),
            package_id: self.package_id.to_string(),
            author_subject: self.author_subject,
            rating: self.rating,
            comment: self.comment,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        })
    }
}
