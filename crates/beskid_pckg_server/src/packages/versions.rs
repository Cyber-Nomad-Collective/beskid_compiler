use super::contracts::PackageVersionPath;
use super::mapping::{now, package_not_found, package_storage_failure, version_summary};
use super::{
    ApiErrorResponse, AppState, HeaderMap, IntoResponse, Json, PackageVersionLifecycleResponse, Path, Response, State,
    StatusCode, StoreError, authenticated_subject,
};

/// Lists version summaries without forcing clients to download the full package
/// detail document. Private packages deliberately remain indistinguishable from
/// absent packages for unauthenticated and non-owner callers.
pub async fn list_versions(State(state): State<AppState>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let subject = authenticated_subject(&state, &headers);
    let package = match state.packages.find_package(&name).await {
        Ok(Some(package)) => package,
        Ok(None) => return package_not_found(),
        Err(_) => return package_storage_failure(),
    };
    if !package.is_public && subject.as_deref() != Some(&package.owner_subject) {
        return package_not_found();
    }
    match state.packages.list_versions(&package.id).await {
        Ok(versions) => {
            Json(versions.iter().map(|version| version_summary(&package, version)).collect::<Vec<_>>()).into_response()
        }
        Err(_) => package_storage_failure(),
    }
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
    match state.packages.set_yanked(&package.id, &version, yanked, now()).await {
        Ok(version) => Json(PackageVersionLifecycleResponse {
            success: true,
            message: if yanked { "version yanked" } else { "version unyanked" }.to_owned(),
            version: Some(version_summary(&package, &version)),
        })
        .into_response(),
        Err(StoreError::VersionNotFound) => package_not_found(),
        Err(StoreError::VersionAlreadyYanked | StoreError::VersionNotYanked) => {
            (StatusCode::CONFLICT, Json(ApiErrorResponse::new("version already has requested lifecycle state")))
                .into_response()
        }
        Err(_) => {
            (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid version lifecycle request"))).into_response()
        }
    }
}
