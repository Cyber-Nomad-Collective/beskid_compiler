use super::contracts::PackageVersionPath;
use super::mapping::{latest_non_yanked, next_id, now, package_not_found, package_storage_failure, version_summary};
use super::{
    ApiErrorResponse, AppState, Body, Bytes, HeaderMap, IntoResponse, Json, NewRegistryActivity, PackageArtifactStore,
    Path, PublishOutcome, PublishRequest, PublishVersion, Response, State, StatusCode, StoreError,
    authenticated_subject, header, validate_package_artifact,
};

pub(super) async fn persist_uploaded_artifact(
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
        if !existing.checksum_sha256.eq_ignore_ascii_case(&validated.checksum_sha256) {
            return (StatusCode::CONFLICT, Json(ApiErrorResponse::new("package version is immutable"))).into_response();
        }
        if !matches!(state.artifacts.verify(&existing.storage_key, &existing.checksum_sha256), Ok(true))
            && state.artifacts.save(PublishRequest { validated, bytes: &bytes }).is_err()
        {
            return artifact_storage_failure();
        }
        return (StatusCode::OK, Json(version_summary(&package, &existing))).into_response();
    }
    let stored = match state.artifacts.save(PublishRequest { validated, bytes: &bytes }) {
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
        Ok(PublishOutcome::Created(version)) => {
            let _ = record_publish_activity(&state, &subject, &package.name, &version.version).await;
            (StatusCode::CREATED, Json(version_summary(&package, &version))).into_response()
        }
        Ok(PublishOutcome::AlreadyExists(version)) => {
            (StatusCode::OK, Json(version_summary(&package, &version))).into_response()
        }
        Err(StoreError::VersionImmutable) => {
            (StatusCode::CONFLICT, Json(ApiErrorResponse::new("package version is immutable"))).into_response()
        }
        Err(_) => {
            (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid package version request"))).into_response()
        }
    }
}

fn artifact_storage_failure() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorResponse::new("artifact storage failed"))).into_response()
}

pub(super) async fn record_publish_activity(
    state: &AppState,
    subject: &str,
    package_name: &str,
    version: &str,
) -> Result<(), ()> {
    state
        .operations
        .append_activity(NewRegistryActivity {
            occurred_at_unix_seconds: now(),
            severity: "Information".to_owned(),
            action: "publish_success".to_owned(),
            message: "Package version published.".to_owned(),
            trace_id: None,
            actor_subject: Some(subject.to_owned()),
            package_name: Some(package_name.to_owned()),
            version: Some(version.to_owned()),
        })
        .await
        .map(|_| ())
        .map_err(|_| ())
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
            return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid package artifact"))).into_response();
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
    let stored_version = match state.packages.find_version(&package.id, &resolved_version).await {
        Ok(version) => version,
        Err(_) => return package_storage_failure(),
    };
    let Some(version) = stored_version else {
        return package_not_found();
    };
    if version.is_yanked {
        return package_not_found();
    }
    match state.artifacts.verify(&version.storage_key, &version.checksum_sha256) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            return (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("package artifact not found"))).into_response();
        }
    }
    let bytes = match state.artifacts.open(&version.storage_key) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("package artifact not found"))).into_response();
        }
    };
    let mut response = Body::from(bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/vnd.beskid.package"));
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, header::HeaderValue::from_static("attachment; filename=package.bpk"));
    response.headers_mut().insert(
        "x-checksum-sha256",
        header::HeaderValue::from_str(&version.checksum_sha256).expect("validated checksum is a valid response header"),
    );
    let _ = package;
    response
}
