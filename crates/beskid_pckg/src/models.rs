use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthActionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginUserRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "rememberMe")]
    pub remember_me: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterUserRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "confirmPassword")]
    pub confirm_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInitialAdminRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "confirmPassword")]
    pub confirm_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapStatusResponse {
    #[serde(rename = "hasUsers")]
    pub has_users: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    #[serde(rename = "isAuthenticated")]
    pub is_authenticated: bool,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    pub email: Option<String>,
    #[serde(rename = "isPublisher")]
    pub is_publisher: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyView {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    #[serde(rename = "createdAtUtc")]
    pub created_at_utc: String,
    #[serde(rename = "revokedAtUtc")]
    pub revoked_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeApiKeyResponse {
    pub success: bool,
    pub message: String,
    #[serde(rename = "revokedAtUtc")]
    pub revoked_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub success: bool,
    #[serde(rename = "plainTextKey")]
    pub plain_text_key: Option<String>,
    pub key: Option<ApiKeyView>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeysListResponse {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    #[serde(rename = "createdAtUtc")]
    pub created_at_utc: String,
    #[serde(rename = "revokedAtUtc")]
    pub revoked_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishPackageVersionResponse {
    pub success: bool,
    pub message: String,
    pub version: Option<PackageVersionSummaryResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertPackageResponse {
    pub success: bool,
    pub message: String,
    pub package: Option<PackageSummaryResponse>,
    #[serde(rename = "reviewId")]
    pub review_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReviewResponse {
    pub id: String,
    #[serde(rename = "packageId")]
    pub package_id: String,
    #[serde(rename = "packageName")]
    pub package_name: String,
    #[serde(rename = "requestedByUserId")]
    pub requested_by_user_id: String,
    pub reason: String,
    pub status: String,
    #[serde(rename = "submittedAtUtc")]
    pub submitted_at_utc: String,
    #[serde(rename = "reviewerUserId")]
    pub reviewer_user_id: Option<String>,
    #[serde(rename = "reviewNotes")]
    pub review_notes: Option<String>,
    #[serde(rename = "reviewedAtUtc")]
    pub reviewed_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewActionRequest {
    #[serde(rename = "reviewId")]
    pub review_id: String,
    pub action: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewActionResponse {
    pub success: bool,
    pub message: String,
    pub review: Option<PackageReviewResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDependencyResponse {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    pub registry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSearchResponse {
    pub package: PackageSummaryResponse,
    #[serde(rename = "reviewCount")]
    pub review_count: i32,
    pub health: PackageHealthSnapshotResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDetailsResponse {
    pub package: PackageSummaryResponse,
    pub versions: Vec<PackageVersionSummaryResponse>,
    pub dependencies: Vec<PackageDependencyResponse>,
    #[serde(rename = "dependentsCount")]
    pub dependents_count: i32,
    pub readme: Option<String>,
    pub health: PackageHealthSnapshotResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVersionLifecycleResponse {
    pub success: bool,
    pub message: String,
    pub version: Option<PackageVersionSummaryResponse>,
}
