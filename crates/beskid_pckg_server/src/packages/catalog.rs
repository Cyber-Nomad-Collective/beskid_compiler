use super::contracts::{DeletePackageResponse, ListQuery, PublisherResponse};
use super::mapping::{
    health, latest_non_yanked, next_id, now, package_not_found, package_storage_failure, package_summary,
    version_summary,
};
use super::{
    ApiErrorResponse, AppState, HeaderMap, IntoResponse, Json, NewPackage, PackageArtifactStore,
    PackageDetailsResponse, PackageSearchResponse, Path, Query, Response, State, StatusCode, StoreError,
    UpsertPackageRequest, authenticated_subject,
};
use beskid_pckg_store::AsyncAdministrationRepository;

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
    let packages = match state.packages.list_packages(query.limit(), query.offset()).await {
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
        .filter(|package| needle.as_ref().is_none_or(|needle| package.name.to_ascii_lowercase().contains(needle)))
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
    let packages = match state.packages.list_packages(query.limit(), query.offset()).await {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    let needle = query.query();
    let results = packages
        .into_iter()
        .filter(|package| package.is_public || subject.as_deref() == Some(&package.owner_subject))
        .filter(|package| needle.as_ref().is_none_or(|needle| package.name.to_ascii_lowercase().contains(needle)))
        .map(|package| PackageSearchResponse { package: package_summary(&package), review_count: 0, health: health() })
        .collect::<Vec<_>>();
    Json(results).into_response()
}

/// Public publisher directory. A publisher is any subject owning at least one
/// public package. Profile metadata (display name, bio, social links) is owned
/// by the community surface (NodeBB) and is no longer projected by the
/// registry; the directory returns the subject as its display name and empty
/// profile fields. Publisher verification is read from the administration
/// store. Private packages never affect the directory or its package counts.
pub async fn list_publishers(State(state): State<AppState>) -> Response {
    let packages = match state.packages.list_packages(200, 0).await {
        Ok(packages) => packages,
        Err(_) => return package_storage_failure(),
    };
    let mut public_counts = std::collections::BTreeMap::<String, usize>::new();
    for package in packages.into_iter().filter(|package| package.is_public) {
        *public_counts.entry(package.owner_subject).or_default() += 1;
    }
    let verification: std::collections::BTreeMap<String, bool> = match &state.api_keys {
        Some(repository) => match repository.list_publisher_verifications().await {
            Ok(verifications) => verifications.into_iter().map(|v| (v.subject, v.is_verified)).collect(),
            Err(_) => return package_storage_failure(),
        },
        None => std::collections::BTreeMap::new(),
    };
    let mut publishers = Vec::new();
    for (subject, package_count) in public_counts {
        publishers.push(PublisherResponse {
            display_name: subject.clone(),
            is_publisher_verified: verification.get(&subject).copied().unwrap_or(false),
            subject,
            bio: String::new(),
            social_links: Vec::new(),
            package_count,
        });
    }
    Json(publishers).into_response()
}

/// Public packages owned by a subject. A subject with no public packages is
/// indistinguishable from a missing publisher, avoiding leakage of private
/// owner subjects.
pub async fn publisher_packages(State(state): State<AppState>, Path(subject): Path<String>) -> Response {
    if !is_valid_subject(&subject) {
        return package_not_found();
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

fn is_valid_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    !trimmed.is_empty()
        && trimmed == subject
        && trimmed.bytes().all(|byte| byte.is_ascii_graphic())
        && trimmed.len() <= 255
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
    let Some(package) =
        package.filter(|package| package.is_public || subject.as_deref() == Some(&package.owner_subject))
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
        versions: versions.iter().map(|version| version_summary(&package, version)).collect(),
        dependencies: Vec::new(),
        dependents_count: 0,
        readme: None,
        health: health(),
        latest_version,
    })
    .into_response()
}

/// Deletes a package only for its Auth Hub owner. The storage boundary commits
/// first; artifact cleanup is best-effort afterwards because it cannot safely
/// be part of a PostgreSQL transaction.
pub async fn delete_package(State(state): State<AppState>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(Some(package)) => package,
        Ok(None) => return package_not_found(),
        Err(_) => return package_storage_failure(),
    };
    if package.owner_subject != subject {
        return package_not_found();
    }
    let removed = match state.packages.delete_package(&name).await {
        Ok(versions) => versions,
        Err(StoreError::PackageNotFound) => return package_not_found(),
        Err(_) => return package_storage_failure(),
    };
    for version in removed {
        let _ = state.artifacts.delete(&version.storage_key);
    }
    Json(DeletePackageResponse { success: true, message: "package deleted".to_owned() }).into_response()
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
        Err(StoreError::PackageAlreadyExists) => {
            (StatusCode::CONFLICT, Json(ApiErrorResponse::new("package already exists"))).into_response()
        }
        Err(_) => (StatusCode::BAD_REQUEST, Json(ApiErrorResponse::new("invalid package request"))).into_response(),
    }
}
