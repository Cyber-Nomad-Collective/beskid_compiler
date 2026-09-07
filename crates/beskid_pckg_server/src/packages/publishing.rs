use super::artifacts::persist_uploaded_artifact;
use super::{
    ApiErrorResponse, AppState, HeaderMap, IntoResponse, Json, Path, Request, Response, State, StatusCode,
    authenticated_publisher_subject, header, validate_package_artifact,
};

pub async fn publish_version(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
    request: Request,
) -> impl IntoResponse {
    if !request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("multipart/form-data"))
    {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(ApiErrorResponse::new("package publication requires multipart/form-data")),
        )
            .into_response();
    }
    publish_multipart_version(state, headers, name, request).await
}

async fn publish_multipart_version(state: AppState, headers: HeaderMap, name: String, request: Request) -> Response {
    let Some(subject) = authenticated_publisher_subject(&state, &headers).await else {
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
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse::new(format!("invalid package artifact: {error}"))),
            )
                .into_response();
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
