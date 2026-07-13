//! HTTP package-registry routes backed by the pckg persistence boundary.

use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{PackageArtifactStore, PublishRequest, validate_package_artifact};
use beskid_pckg_contract::{
    ApiErrorResponse, PackageDetailsResponse, PackageHealthSnapshotResponse,
    PackageSummaryResponse, PackageVersionLifecycleResponse, PackageVersionSummaryResponse,
    PublishPackageVersionRequest, UpsertPackageRequest,
};
use beskid_pckg_store::{
    NewPackage, Package, PackageRepository, PackageVersion, PublishOutcome, PublishVersion,
    StoreError,
};

use crate::{AppState, authenticated_subject};

static NEXT_IDENTIFIER: AtomicU64 = AtomicU64::new(1);

pub async fn list_packages(State(state): State<AppState>) -> Json<Vec<PackageSummaryResponse>> {
    // The repository deliberately does not expose enumeration until its SQL
    // adapter can apply search/paging consistently. Keep the list endpoint
    // stable while named package routes become functional.
    let _ = state;
    Json(Vec::new())
}

pub async fn package_detail(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let subject = authenticated_subject(&state, &headers);
    let package = state
        .packages
        .lock()
        .expect("package repository mutex is not poisoned")
        .find_package(&name)
        .cloned();
    let Some(package) = package
        .filter(|package| package.is_public || subject.as_deref() == Some(&package.owner_subject))
    else {
        return package_not_found();
    };

    Json(PackageDetailsResponse {
        package: package_summary(&package),
        versions: Vec::new(),
        dependencies: Vec::new(),
        dependents_count: 0,
        readme: None,
        health: health(),
        latest_version: None,
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
    let mut packages = state
        .packages
        .lock()
        .expect("package repository mutex is not poisoned");
    match packages.create_package(NewPackage {
        id: next_id("package"),
        name: request.name,
        owner_subject: subject,
        is_public: request.is_public,
        now_unix_seconds: now(),
    }) {
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
    publish_version_metadata(state, headers, name, request)
}

fn publish_version_metadata(
    state: AppState,
    headers: axum::http::HeaderMap,
    name: String,
    request: PublishPackageVersionRequest,
) -> axum::response::Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let mut packages = state
        .packages
        .lock()
        .expect("package repository mutex is not poisoned");
    let Some(package) = packages.find_package(&name).cloned() else {
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
    match packages.publish_version(PublishVersion {
        id: next_id("version"),
        package_id: package.id.clone(),
        storage_key: format!("packages/{}/{}.bpk", package.id, version),
        version,
        checksum_sha256,
        size_bytes: 0,
        now_unix_seconds: now(),
    }) {
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
    persist_uploaded_artifact(state, subject, name, version, artifact, validated)
}

fn persist_uploaded_artifact(
    state: AppState,
    subject: String,
    name: String,
    version: String,
    bytes: Bytes,
    validated: beskid_pckg_artifacts::ValidatedArtifact,
) -> Response {
    let mut packages = state
        .packages
        .lock()
        .expect("package repository mutex is not poisoned");
    let Some(package) = packages.find_package(&name).cloned() else {
        return package_not_found();
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    if let Some(existing) = packages.find_version(&package.id, &version).cloned() {
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
    match packages.publish_version(PublishVersion {
        id: next_id("version"),
        package_id: package.id.clone(),
        version,
        checksum_sha256: stored.checksum_sha256,
        storage_key: stored.storage_key,
        size_bytes: stored.size_bytes,
        now_unix_seconds: now(),
    }) {
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

/// Publishes a validated `.bpk` artifact as a raw request body.
///
/// The legacy form endpoint carried metadata and artifact bytes together. This
/// route makes the immutable version segment explicit so the server can
/// validate the archive before writing it and use the computed checksum as the
/// one source of truth.
pub async fn upload_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((name, version)): Path<(String, String)>,
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
    persist_uploaded_artifact(state, subject, name, version, bytes, validated)
}

/// Serves a verified package artifact. Private packages retain the registry's
/// not-found behavior for non-owners; yanked versions are never downloadable.
pub async fn download_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((name, version)): Path<(String, String)>,
) -> Response {
    let subject = authenticated_subject(&state, &headers);
    let (package, version) = {
        let packages = state
            .packages
            .lock()
            .expect("package repository mutex is not poisoned");
        let Some(package) = packages.find_package(&name).cloned() else {
            return package_not_found();
        };
        if !package.is_public && subject.as_deref() != Some(&package.owner_subject) {
            return package_not_found();
        }
        let Some(version) = packages.find_version(&package.id, &version).cloned() else {
            return package_not_found();
        };
        if version.is_yanked {
            return package_not_found();
        }
        (package, version)
    };
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
    Path((name, version)): Path<(String, String)>,
) -> impl IntoResponse {
    set_yanked(state, headers, name, version, true).await
}

pub async fn unyank_version(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path((name, version)): Path<(String, String)>,
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
    let mut packages = state
        .packages
        .lock()
        .expect("package repository mutex is not poisoned");
    let Some(package) = packages.find_package(&name).cloned() else {
        return package_not_found();
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    match packages.set_yanked(&package.id, &version, yanked, now()) {
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
fn next_id(kind: &str) -> String {
    format!("{kind}-{}", NEXT_IDENTIFIER.fetch_add(1, Ordering::Relaxed))
}
