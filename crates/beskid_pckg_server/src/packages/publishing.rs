use super::artifacts::{persist_uploaded_artifact, record_publish_activity};
use super::mapping::{next_id, now, package_not_found, package_storage_failure, version_summary};
use super::{
    ApiErrorResponse, AppState, HeaderMap, IntoResponse, Json, Path, PublishOutcome, PublishPackageVersionRequest,
    PublishVersion, Request, Response, State, StatusCode, StoreError, authenticated_subject, header, to_bytes,
};

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
            return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid package version request")))
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
        return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("version and checksumSha256 are required")))
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

async fn publish_multipart_version(state: AppState, headers: HeaderMap, name: String, request: Request) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let content_type = match request.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()) {
        Some(content_type) => content_type,
        None => return invalid_publish_form(),
    };
    let boundary = match multer::parse_boundary(content_type) {
        Ok(boundary) => boundary,
        Err(_) => return invalid_publish_form(),
    };
    let constraints =
        multer::Constraints::new().allowed_fields(vec!["version", "checksumSha256", "artifact"]).size_limit(
            multer::SizeLimit::new()
                .whole_stream(64 * 1024 * 1024)
                .per_field(64 * 1024 * 1024)
                .for_field("version", 128)
                .for_field("checksumSha256", 128),
        );
    let mut form = multer::Multipart::with_constraints(request.into_body().into_data_stream(), boundary, constraints);
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
            Some("version") if version.is_none() => version = String::from_utf8(bytes.to_vec()).ok(),
            Some("checksumSha256") if checksum.is_none() => checksum = String::from_utf8(bytes.to_vec()).ok(),
            Some("artifact") if artifact.is_none() => artifact = Some(bytes),
            _ => return invalid_publish_form(),
        }
    }
    let (Some(version), Some(checksum), Some(artifact)) = (version, checksum, artifact) else {
        return invalid_publish_form();
    };
    let validated = match validate_package_artifact(&artifact, &name, &version) {
        Ok(validated) if validated.checksum_sha256.eq_ignore_ascii_case(checksum.trim()) => validated,
        Ok(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new("artifact checksum does not match checksumSha256")),
            )
                .into_response();
        }
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid package artifact"))).into_response();
        }
    };
    persist_uploaded_artifact(state, subject, name, version, artifact, validated).await
}

fn invalid_publish_form() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiErrorResponse::new("multipart publish requires version, checksumSha256, and artifact")),
    )
        .into_response()
}
