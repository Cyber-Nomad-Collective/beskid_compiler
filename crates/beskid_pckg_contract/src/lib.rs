//! HTTP contracts shared by the pckg compatibility server and its consumers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

impl HealthResponse {
    pub const fn ok() -> Self {
        Self { status: "ok" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiErrorResponse {
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionResponse {
    pub subject: String,
    #[serde(rename = "githubLogin")]
    pub github_login: String,
    #[serde(rename = "hubSessionId")]
    pub hub_session_id: String,
}

impl ApiErrorResponse {
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

/// Legacy package-list payload retained while the registry API moves to Rust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSummaryResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: Option<String>,
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    pub tags: Vec<String>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    #[serde(rename = "totalDownloads")]
    pub total_downloads: i64,
    #[serde(rename = "updatedAtUtc")]
    pub updated_at_utc: String,
    #[serde(rename = "pendingReviewsCount")]
    pub pending_reviews_count: i32,
    #[serde(rename = "averageRating")]
    pub average_rating: f64,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "ownerUserId")]
    pub owner_user_id: String,
    #[serde(rename = "ownerDisplayName")]
    pub owner_display_name: String,
    #[serde(rename = "ownerIsPublisherVerified")]
    pub owner_is_publisher_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageVersionSummaryResponse {
    pub id: String,
    #[serde(rename = "packageId")]
    pub package_id: String,
    #[serde(rename = "packageName")]
    pub package_name: String,
    pub version: String,
    #[serde(rename = "isYanked")]
    pub is_yanked: bool,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: i64,
    #[serde(rename = "publishedAtUtc")]
    pub published_at_utc: String,
    #[serde(rename = "yankedAtUtc")]
    pub yanked_at_utc: Option<String>,
    #[serde(rename = "hasReadme")]
    pub has_readme: bool,
    #[serde(rename = "configuration", serialize_with = "serialize_optional_json")]
    pub configuration_json: Option<String>,
    #[serde(rename = "overrides", serialize_with = "serialize_optional_json")]
    pub overrides_json: Option<String>,
}

fn serialize_optional_json<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(value) => match serde_json::from_str::<Value>(value) {
            Ok(json) => json.serialize(serializer),
            Err(_) => value.serialize(serializer),
        },
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageHealthSnapshotResponse {
    pub state: String,
    #[serde(rename = "subState")]
    pub sub_state: String,
    pub score: f64,
    #[serde(rename = "updateRateState")]
    pub update_rate_state: String,
    #[serde(rename = "updateRateSubState")]
    pub update_rate_sub_state: String,
    #[serde(rename = "updateRateNormalized")]
    pub update_rate_normalized: f64,
    #[serde(rename = "updateRateWeight")]
    pub update_rate_weight: f64,
    #[serde(rename = "downloadsState")]
    pub downloads_state: String,
    #[serde(rename = "downloadsSubState")]
    pub downloads_sub_state: String,
    #[serde(rename = "downloadsNormalized")]
    pub downloads_normalized: f64,
    #[serde(rename = "downloadsWeight")]
    pub downloads_weight: f64,
    #[serde(rename = "reviewsState")]
    pub reviews_state: String,
    #[serde(rename = "reviewsSubState")]
    pub reviews_sub_state: String,
    #[serde(rename = "reviewsNormalized")]
    pub reviews_normalized: f64,
    #[serde(rename = "reviewsWeight")]
    pub reviews_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSearchResponse {
    pub package: PackageSummaryResponse,
    #[serde(rename = "reviewCount")]
    pub review_count: i32,
    pub health: PackageHealthSnapshotResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageDetailsResponse {
    pub package: PackageSummaryResponse,
    pub versions: Vec<PackageVersionSummaryResponse>,
    pub dependencies: Vec<PackageDependencyResponse>,
    #[serde(rename = "dependentsCount")]
    pub dependents_count: i32,
    pub readme: Option<String>,
    pub health: PackageHealthSnapshotResponse,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDependencyResponse {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    pub registry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageVersionLifecycleResponse {
    pub success: bool,
    pub message: String,
    pub version: Option<PackageVersionSummaryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishPackageVersionRequest {
    pub version: Option<String>,
    #[serde(rename = "versionBump")]
    pub version_bump: Option<String>,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: Option<String>,
}

impl PublishPackageVersionRequest {
    pub fn is_idempotent_against(&self, existing: &PackageVersionSummaryResponse) -> bool {
        self.checksum_sha256.as_deref() == Some(existing.checksum_sha256.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertPackageRequest {
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: Option<String>,
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    #[serde(rename = "submitForReview")]
    pub submit_for_review: bool,
    #[serde(rename = "reviewReason")]
    pub review_reason: Option<String>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackageDownloadContract {
    pub version: PackageVersionSummaryResponse,
    pub content_type: String,
    pub content_disposition: String,
}

#[derive(Debug, Clone)]
pub struct PackageContractFixture {
    package: PackageSummaryResponse,
    versions: Vec<PackageVersionSummaryResponse>,
    health: PackageHealthSnapshotResponse,
}

impl PackageContractFixture {
    pub fn new(
        package: PackageSummaryResponse,
        versions: Vec<PackageVersionSummaryResponse>,
        health: PackageHealthSnapshotResponse,
    ) -> Self {
        Self { package, versions, health }
    }

    pub fn detail_for(&self, subject: Option<&str>) -> Option<PackageDetailsResponse> {
        if !self.is_visible_to(subject) {
            return None;
        }

        Some(PackageDetailsResponse {
            package: self.package.clone(),
            versions: self.versions.clone(),
            dependencies: Vec::new(),
            dependents_count: 0,
            readme: None,
            health: self.health.clone(),
            latest_version: self.latest_active().map(|version| version.version.clone()),
        })
    }

    pub fn download_for(&self, subject: Option<&str>, requested_version: &str) -> Option<PackageDownloadContract> {
        if !self.is_visible_to(subject) {
            return None;
        }

        let version = if requested_version == "latest" {
            self.latest_active()
        } else {
            self.versions.iter().find(|version| version.version == requested_version && !version.is_yanked)
        }?;

        Some(PackageDownloadContract {
            content_type: "application/zip".to_owned(),
            content_disposition: format!("attachment; filename={}-{}.bpk", self.package.name, version.version),
            version: version.clone(),
        })
    }

    fn is_visible_to(&self, subject: Option<&str>) -> bool {
        self.package.is_public || subject == Some(self.package.owner_user_id.as_str())
    }

    fn latest_active(&self) -> Option<&PackageVersionSummaryResponse> {
        self.versions
            .iter()
            .filter(|version| !version.is_yanked)
            .max_by(|left, right| left.published_at_utc.cmp(&right.published_at_utc))
    }
}
