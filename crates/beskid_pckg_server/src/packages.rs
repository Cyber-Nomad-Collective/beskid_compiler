//! HTTP package-registry routes backed by the pckg persistence boundary.

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{
    ArtifactRecord, PackageArtifactStore, PublishRequest, select_download,
    validate_package_artifact,
};
use beskid_pckg_contract::{
    ApiErrorResponse, PackageDetailsResponse, PackageHealthSnapshotResponse, PackageSearchResponse,
    PackageSummaryResponse, PackageVersionLifecycleResponse, PackageVersionSummaryResponse,
    PublishPackageVersionRequest, UpsertPackageRequest,
};
use beskid_pckg_store::{
    NewPackage, Package, PackageVersion, PublishOutcome, PublishVersion, StoreError,
};

use crate::{AppState, authenticated_subject};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PublisherResponse {
    subject: String,
    display_name: String,
    bio: String,
    social_links: Vec<String>,
    is_publisher_verified: bool,
    package_count: usize,
}

#[derive(serde::Deserialize)]
pub(crate) struct PackageVersionPath {
    name: String,
    version: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct ListQuery {
    q: Option<String>,
    owner: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    page: Option<i64>,
}

impl ListQuery {
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(100).clamp(1, 200)
    }

    fn offset(&self) -> i64 {
        self.offset
            .unwrap_or_else(|| self.page.unwrap_or(0).max(0).saturating_mul(self.limit()))
            .max(0)
    }

    fn query(&self) -> Option<String> {
        self.q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
    }

    fn requests_current_owner(&self) -> bool {
        self.owner.as_deref() == Some("me")
    }
}

/// Legacy package index. Results are visibility-filtered before paging so a
/// private package never leaks through a count or a later page.
pub async fn list_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    let subject = authenticated_subject(&state, &headers);
    if query.requests_current_owner() && subject.is_none() {
        return crate::unauthorized_response();
    }
    let packages = match state
        .packages
        .list_packages(query.limit(), query.offset())
        .await
    {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    let needle = query.query();
    let summaries = packages
        .into_iter()
        .filter(|package| {
            if query.requests_current_owner() {
                subject.as_deref() == Some(&package.owner_subject)
            } else {
                package.is_public || subject.as_deref() == Some(&package.owner_subject)
            }
        })
        .filter(|package| {
            needle
                .as_ref()
                .is_none_or(|needle| package.name.to_ascii_lowercase().contains(needle))
        })
        .map(|package| package_summary(&package))
        .collect::<Vec<_>>();
    Json(summaries).into_response()
}

/// Search keeps the historic `/api/search` payload shape while using the same
/// visibility and paging rules as the package index.
pub async fn search_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    let subject = authenticated_subject(&state, &headers);
    let packages = match state
        .packages
        .list_packages(query.limit(), query.offset())
        .await
    {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    let needle = query.query();
    let results = packages
        .into_iter()
        .filter(|package| package.is_public || subject.as_deref() == Some(&package.owner_subject))
        .filter(|package| {
            needle
                .as_ref()
                .is_none_or(|needle| package.name.to_ascii_lowercase().contains(needle))
        })
        .map(|package| PackageSearchResponse {
            package: package_summary(&package),
            review_count: 0,
            health: health(),
        })
        .collect::<Vec<_>>();
    Json(results).into_response()
}

/// Public publisher directory. A publisher must have both an Auth-Hub-subject
/// keyed community profile and at least one public package. Private packages
/// never affect the directory or its package counts.
pub async fn list_publishers(State(state): State<AppState>) -> Response {
    let packages = match state.packages.list_packages(200, 0).await {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    let mut public_counts = std::collections::BTreeMap::<String, usize>::new();
    for package in packages.into_iter().filter(|package| package.is_public) {
        *public_counts.entry(package.owner_subject).or_default() += 1;
    }
    let mut publishers = Vec::new();
    for (subject, package_count) in public_counts {
        match state.community.profile_for_catalog(&subject).await {
            Ok(Some(profile)) => publishers.push(PublisherResponse {
                subject: profile.subject,
                display_name: profile.display_name,
                bio: profile.bio,
                social_links: profile.social_links,
                is_publisher_verified: profile.is_publisher_verified,
                package_count,
            }),
            Ok(None) => {}
            Err(_) => return package_storage_failure(),
        }
    }
    Json(publishers).into_response()
}

/// Public packages owned by a profile-backed Auth Hub subject. A missing
/// profile is indistinguishable from a missing publisher, avoiding leakage of
/// package owner subjects which have not opted into the directory.
pub async fn publisher_packages(
    State(state): State<AppState>,
    Path(subject): Path<String>,
) -> Response {
    if !is_github_subject(&subject) {
        return package_not_found();
    }
    match state.community.profile_for_catalog(&subject).await {
        Ok(Some(_)) => {}
        Ok(None) => return package_not_found(),
        Err(_) => return package_storage_failure(),
    }
    let packages = match state.packages.list_packages(200, 0).await {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    Json(
        packages
            .into_iter()
            .filter(|package| package.is_public && package.owner_subject == subject)
            .map(|package| package_summary(&package))
            .collect::<Vec<_>>(),
    )
    .into_response()
}

fn is_github_subject(subject: &str) -> bool {
    subject
        .strip_prefix("github:")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
}

pub async fn package_detail(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let subject = authenticated_subject(&state, &headers);
    let package = match state.packages.find_package(&name).await {
        Ok(package) => package,
        Err(_) => return package_storage_failure(),
    };
    let package = match package {
        Some(package) => Some(package),
        None => match state.packages.find_package_by_id(&name).await {
            Ok(package) => package,
            Err(_) => return package_storage_failure(),
        },
    };
    let Some(package) = package
        .filter(|package| package.is_public || subject.as_deref() == Some(&package.owner_subject))
    else {
        return package_not_found();
    };

    let versions = match state.packages.list_versions(&package.id).await {
        Ok(versions) => versions,
        Err(_) => return package_storage_failure(),
    };
    let latest_version = latest_non_yanked(&versions).map(|version| version.version.clone());
    Json(PackageDetailsResponse {
        package: package_summary(&package),
        versions: versions
            .iter()
            .map(|version| version_summary(&package, version))
            .collect(),
        dependencies: Vec::new(),
        dependents_count: 0,
        readme: None,
        health: health(),
        latest_version,
    })
    .into_response()
}

pub async fn upsert_package(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpsertPackageRequest>,
) -> impl IntoResponse {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    match state
        .packages
        .create_package(NewPackage {
            id: next_id("package"),
            name: request.name,
            owner_subject: subject,
            is_public: request.is_public,
            now_unix_seconds: now(),
        })
        .await
    {
        Ok(package) => (StatusCode::CREATED, Json(package_summary(&package))).into_response(),
        Err(StoreError::PackageAlreadyExists) => (
            StatusCode::CONFLICT,
            Json(ApiErrorResponse::new("package already exists")),
        )
            .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new("invalid package request")),
        )
            .into_response(),
    }
}

pub async fn publish_version(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("multipart/form-data"))
    {
        return publish_multipart_version(state, headers, name, request).await;
    }
    let request = match to_bytes(request.into_body(), 64 * 1024 * 1024)
        .await
        .ok()
        .and_then(|body| serde_json::from_slice::<PublishPackageVersionRequest>(&body).ok())
    {
        Some(request) => request,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new("invalid package version request")),
            )
                .into_response();
        }
    };
    publish_version_metadata(state, headers, name, request).await
}

async fn publish_version_metadata(
    state: AppState,
    headers: axum::http::HeaderMap,
    name: String,
    request: PublishPackageVersionRequest,
) -> axum::response::Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(package) => package,
        Err(_) => return package_storage_failure(),
    };
    let Some(package) = package else {
        return package_not_found();
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    let (Some(version), Some(checksum_sha256)) = (request.version, request.checksum_sha256) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new(
                "version and checksumSha256 are required",
            )),
        )
            .into_response();
    };
    match state
        .packages
        .publish_version(PublishVersion {
            id: next_id("version"),
            package_id: package.id.clone(),
            storage_key: format!("packages/{}/{}.bpk", package.id, version),
            version,
            checksum_sha256,
            size_bytes: 0,
            now_unix_seconds: now(),
        })
        .await
    {
        Ok(PublishOutcome::Created(version)) => (
            StatusCode::CREATED,
            Json(version_summary(&package, &version)),
        )
            .into_response(),
        Ok(PublishOutcome::AlreadyExists(version)) => {
            (StatusCode::OK, Json(version_summary(&package, &version))).into_response()
        }
        Err(StoreError::VersionImmutable) => (
            StatusCode::CONFLICT,
            Json(ApiErrorResponse::new("package version is immutable")),
        )
            .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new("invalid package version request")),
        )
            .into_response(),
    }
}

async fn publish_multipart_version(
    state: AppState,
    headers: HeaderMap,
    name: String,
    request: Request,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let content_type = match request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        Some(content_type) => content_type,
        None => return invalid_publish_form(),
    };
    let boundary = match multer::parse_boundary(content_type) {
        Ok(boundary) => boundary,
        Err(_) => return invalid_publish_form(),
    };
    let constraints = multer::Constraints::new()
        .allowed_fields(vec!["version", "checksumSha256", "artifact"])
        .size_limit(
            multer::SizeLimit::new()
                .whole_stream(64 * 1024 * 1024)
                .per_field(64 * 1024 * 1024)
                .for_field("version", 128)
                .for_field("checksumSha256", 128),
        );
    let mut form = multer::Multipart::with_constraints(
        request.into_body().into_data_stream(),
        boundary,
        constraints,
    );
    let mut version = None;
    let mut checksum = None;
    let mut artifact = None;
    loop {
        let field = match form.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => return invalid_publish_form(),
        };
        let name = field.name().map(str::to_owned);
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => return invalid_publish_form(),
        };
        match name.as_deref() {
            Some("version") if version.is_none() => {
                version = String::from_utf8(bytes.to_vec()).ok()
            }
            Some("checksumSha256") if checksum.is_none() => {
                checksum = String::from_utf8(bytes.to_vec()).ok()
            }
            Some("artifact") if artifact.is_none() => artifact = Some(bytes),
            _ => return invalid_publish_form(),
        }
    }
    let (Some(version), Some(checksum), Some(artifact)) = (version, checksum, artifact) else {
        return invalid_publish_form();
    };
    let validated = match validate_package_artifact(&artifact, &name, &version) {
        Ok(validated)
            if validated
                .checksum_sha256
                .eq_ignore_ascii_case(checksum.trim()) =>
        {
            validated
        }
        Ok(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new(
                    "artifact checksum does not match checksumSha256",
                )),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new("invalid package artifact")),
            )
                .into_response();
        }
    };
    persist_uploaded_artifact(state, subject, name, version, artifact, validated).await
}

async fn persist_uploaded_artifact(
    state: AppState,
    subject: String,
    name: String,
    version: String,
    bytes: Bytes,
    validated: beskid_pckg_artifacts::ValidatedArtifact,
) -> Response {
    let package = match state.packages.find_package(&name).await {
        Ok(package) => package,
        Err(_) => return package_storage_failure(),
    };
    let Some(package) = package else {
        return package_not_found();
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    let existing = match state.packages.find_version(&package.id, &version).await {
        Ok(existing) => existing,
        Err(_) => return package_storage_failure(),
    };
    if let Some(existing) = existing {
        if !existing
            .checksum_sha256
            .eq_ignore_ascii_case(&validated.checksum_sha256)
        {
            return (
                StatusCode::CONFLICT,
                Json(ApiErrorResponse::new("package version is immutable")),
            )
                .into_response();
        }
        if !matches!(
            state
                .artifacts
                .verify(&existing.storage_key, &existing.checksum_sha256),
            Ok(true)
        ) && state
            .artifacts
            .save(PublishRequest {
                validated,
                bytes: &bytes,
            })
            .is_err()
        {
            return artifact_storage_failure();
        }
        return (StatusCode::OK, Json(version_summary(&package, &existing))).into_response();
    }
    let stored = match state.artifacts.save(PublishRequest {
        validated,
        bytes: &bytes,
    }) {
        Ok(stored) => stored,
        Err(_) => return artifact_storage_failure(),
    };
    match state
        .packages
        .publish_version(PublishVersion {
            id: next_id("version"),
            package_id: package.id.clone(),
            version,
            checksum_sha256: stored.checksum_sha256,
            storage_key: stored.storage_key,
            size_bytes: stored.size_bytes,
            now_unix_seconds: now(),
        })
        .await
    {
        Ok(PublishOutcome::Created(version)) => (
            StatusCode::CREATED,
            Json(version_summary(&package, &version)),
        )
            .into_response(),
        Ok(PublishOutcome::AlreadyExists(version)) => {
            (StatusCode::OK, Json(version_summary(&package, &version))).into_response()
        }
        Err(StoreError::VersionImmutable) => (
            StatusCode::CONFLICT,
            Json(ApiErrorResponse::new("package version is immutable")),
        )
            .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new("invalid package version request")),
        )
            .into_response(),
    }
}

fn invalid_publish_form() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiErrorResponse::new(
            "multipart publish requires version, checksumSha256, and artifact",
        )),
    )
        .into_response()
}

fn artifact_storage_failure() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse::new("artifact storage failed")),
    )
        .into_response()
}

fn package_storage_failure() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiErrorResponse::new("package storage unavailable")),
    )
        .into_response()
}

/// Publishes a validated `.bpk` artifact as a raw request body.
///
/// The legacy form endpoint carried metadata and artifact bytes together. This
/// route makes the immutable version segment explicit so the server can
/// validate the archive before writing it and use the computed checksum as the
/// one source of truth.
pub async fn upload_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PackageVersionPath { name, version }): Path<PackageVersionPath>,
    bytes: Bytes,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let validated = match validate_package_artifact(&bytes, &name, &version) {
        Ok(validated) => validated,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new("invalid package artifact")),
            )
                .into_response();
        }
    };
    persist_uploaded_artifact(state, subject, name, version, bytes, validated).await
}

/// Serves a verified package artifact. Private packages retain the registry's
/// not-found behavior for non-owners; yanked versions are never downloadable.
pub async fn download_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PackageVersionPath { name, version }): Path<PackageVersionPath>,
) -> Response {
    let subject = authenticated_subject(&state, &headers);
    let package = match state.packages.find_package(&name).await {
        Ok(package) => package,
        Err(_) => return package_storage_failure(),
    };
    let Some(package) = package else {
        return package_not_found();
    };
    if !package.is_public && subject.as_deref() != Some(&package.owner_subject) {
        return package_not_found();
    }
    let resolved_version = if version.eq_ignore_ascii_case("latest") {
        let versions = match state.packages.list_versions(&package.id).await {
            Ok(versions) => versions,
            Err(_) => return package_storage_failure(),
        };
        match latest_non_yanked(&versions) {
            Some(version) => version.version.clone(),
            None => return package_not_found(),
        }
    } else {
        version
    };
    let stored_version = match state
        .packages
        .find_version(&package.id, &resolved_version)
        .await
    {
        Ok(version) => version,
        Err(_) => return package_storage_failure(),
    };
    let Some(version) = stored_version else {
        return package_not_found();
    };
    if version.is_yanked {
        return package_not_found();
    }
    match state
        .artifacts
        .verify(&version.storage_key, &version.checksum_sha256)
    {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiErrorResponse::new("package artifact not found")),
            )
                .into_response();
        }
    }
    let bytes = match state.artifacts.open(&version.storage_key) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiErrorResponse::new("package artifact not found")),
            )
                .into_response();
        }
    };
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/vnd.beskid.package"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=package.bpk"),
    );
    response.headers_mut().insert(
        "x-checksum-sha256",
        header::HeaderValue::from_str(&version.checksum_sha256)
            .expect("validated checksum is a valid response header"),
    );
    let _ = package;
    response
}

pub async fn yank_version(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(PackageVersionPath { name, version }): Path<PackageVersionPath>,
) -> impl IntoResponse {
    set_yanked(state, headers, name, version, true).await
}

pub async fn unyank_version(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(PackageVersionPath { name, version }): Path<PackageVersionPath>,
) -> impl IntoResponse {
    set_yanked(state, headers, name, version, false).await
}

async fn set_yanked(
    state: AppState,
    headers: axum::http::HeaderMap,
    name: String,
    version: String,
    yanked: bool,
) -> axum::response::Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(package) => package,
        Err(_) => return package_storage_failure(),
    };
    let Some(package) = package else {
        return package_not_found();
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    match state
        .packages
        .set_yanked(&package.id, &version, yanked, now())
        .await
    {
        Ok(version) => Json(PackageVersionLifecycleResponse {
            success: true,
            message: if yanked {
                "version yanked"
            } else {
                "version unyanked"
            }
            .to_owned(),
            version: Some(version_summary(&package, &version)),
        })
        .into_response(),
        Err(StoreError::VersionNotFound) => package_not_found(),
        Err(StoreError::VersionAlreadyYanked | StoreError::VersionNotYanked) => (
            StatusCode::CONFLICT,
            Json(ApiErrorResponse::new(
                "version already has requested lifecycle state",
            )),
        )
            .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new("invalid version lifecycle request")),
        )
            .into_response(),
    }
}

fn package_summary(package: &Package) -> PackageSummaryResponse {
    PackageSummaryResponse {
        id: package.id.clone(),
        name: package.name.clone(),
        description: String::new(),
        category: "General".to_owned(),
        repository_url: None,
        website_url: None,
        tags: Vec::new(),
        is_public: package.is_public,
        total_downloads: 0,
        updated_at_utc: timestamp(package.updated_at_unix_seconds),
        pending_reviews_count: 0,
        average_rating: 0.0,
        icon_url: None,
        owner_user_id: package.owner_subject.clone(),
        owner_display_name: package.owner_subject.clone(),
        owner_is_publisher_verified: false,
    }
}

fn version_summary(package: &Package, version: &PackageVersion) -> PackageVersionSummaryResponse {
    PackageVersionSummaryResponse {
        id: version.id.clone(),
        package_id: package.id.clone(),
        package_name: package.name.clone(),
        version: version.version.clone(),
        is_yanked: version.is_yanked,
        checksum_sha256: version.checksum_sha256.clone(),
        size_bytes: version.size_bytes as i64,
        published_at_utc: timestamp(version.published_at_unix_seconds),
        yanked_at_utc: version.yanked_at_unix_seconds.map(timestamp),
        has_readme: false,
        configuration_json: None,
        overrides_json: None,
    }
}

fn latest_non_yanked(versions: &[PackageVersion]) -> Option<&PackageVersion> {
    let records = versions
        .iter()
        .map(|version| ArtifactRecord::new(&version.version, version.is_yanked))
        .collect::<Vec<_>>();
    let selected = select_download(&records, "latest")?;
    versions
        .iter()
        .find(|version| version.version == selected.version)
}

fn health() -> PackageHealthSnapshotResponse {
    PackageHealthSnapshotResponse {
        state: "unknown".to_owned(),
        sub_state: "unrated".to_owned(),
        score: 0.0,
        update_rate_state: "unknown".to_owned(),
        update_rate_sub_state: "unrated".to_owned(),
        update_rate_normalized: 0.0,
        update_rate_weight: 0.0,
        downloads_state: "unknown".to_owned(),
        downloads_sub_state: "unrated".to_owned(),
        downloads_normalized: 0.0,
        downloads_weight: 0.0,
        reviews_state: "unknown".to_owned(),
        reviews_sub_state: "unrated".to_owned(),
        reviews_normalized: 0.0,
        reviews_weight: 0.0,
    }
}

fn package_not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorResponse::new("package not found")),
    )
        .into_response()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after Unix epoch")
        .as_secs() as i64
}
fn timestamp(seconds: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .expect("repository timestamps are valid Unix seconds")
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting succeeds")
}
fn next_id(_kind: &str) -> String {
    uuid::Uuid::new_v4().to_string()
}
