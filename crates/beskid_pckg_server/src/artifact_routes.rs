//! Read-only, visibility-preserving views over verified package artifacts.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{
    ArtifactBrowser, ArtifactRecord, BrowseEntry, PackageArtifactStore, ValidatedArtifact, select_download,
};
use beskid_pckg_contract::ApiErrorResponse;
use beskid_pckg_store::{Package, PackageVersion};
use serde::{Deserialize, Serialize};

use crate::{AppState, authenticated_subject};

#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactPath {
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BrowseFileQuery {
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowseEntryResponse {
    path: String,
    size_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredDocumentationResponse {
    readme: Option<String>,
    metadata: Option<serde_json::Value>,
}

pub(crate) async fn list_docs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    match browser.list_docs() {
        Ok(entries) => Json(entries.into_iter().map(entry_response).collect::<Vec<_>>()).into_response(),
        Err(_) => hidden_not_found(),
    }
}

pub(crate) async fn readme(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    text_response(browser.read_doc("README.md"), "text/markdown; charset=utf-8")
}

pub(crate) async fn read_doc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
    Query(query): Query<BrowseFileQuery>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    text_response(
        query.path.as_deref().map_or_else(
            || Err(beskid_pckg_artifacts::ArtifactError::EntryNotFound),
            |requested| browser.read_doc(requested),
        ),
        "text/markdown; charset=utf-8",
    )
}

pub(crate) async fn structured_docs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    match browser.documentation() {
        Ok(documentation) => {
            Json(StructuredDocumentationResponse { readme: documentation.readme, metadata: documentation.metadata })
                .into_response()
        }
        Err(_) => hidden_not_found(),
    }
}

pub(crate) async fn source_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    match browser.list_source_tree() {
        Ok(entries) => Json(entries.into_iter().map(entry_response).collect::<Vec<_>>()).into_response(),
        Err(_) => hidden_not_found(),
    }
}

pub(crate) async fn read_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
    Query(query): Query<BrowseFileQuery>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    text_response(
        query.path.as_deref().map_or_else(
            || Err(beskid_pckg_artifacts::ArtifactError::EntryNotFound),
            |requested| browser.read_source(requested),
        ),
        "text/plain; charset=utf-8",
    )
}

async fn browser_for_request(state: &AppState, headers: &HeaderMap, path: &ArtifactPath) -> Option<ArtifactBrowser> {
    let subject = authenticated_subject(state, headers);
    let package = state.packages.find_package(&path.name).await.ok()??;
    if !package.is_public && subject.as_deref() != Some(&package.owner_subject) {
        return None;
    }
    let version = resolve_version(state, &package, &path.version).await?;
    if !state.artifacts.verify(&version.storage_key, &version.checksum_sha256).ok()? {
        return None;
    }
    let bytes = state.artifacts.open(&version.storage_key).ok()?;
    ArtifactBrowser::from_validated_bytes(
        &bytes,
        &ValidatedArtifact {
            package_name: package.name,
            version: version.version,
            checksum_sha256: version.checksum_sha256,
            size_bytes: version.size_bytes,
            manifest_json: String::new(),
        },
    )
    .ok()
}

async fn resolve_version(state: &AppState, package: &Package, requested: &str) -> Option<PackageVersion> {
    if requested.eq_ignore_ascii_case("latest") {
        let versions = state.packages.list_versions(&package.id).await.ok()?;
        let records =
            versions.iter().map(|version| ArtifactRecord::new(&version.version, version.is_yanked)).collect::<Vec<_>>();
        let selected = select_download(&records, requested)?;
        versions.into_iter().find(|version| version.version == selected.version)
    } else {
        let version = state.packages.find_version(&package.id, requested).await.ok()??;
        (!version.is_yanked).then_some(version)
    }
}

fn entry_response(entry: BrowseEntry) -> BrowseEntryResponse {
    BrowseEntryResponse { path: entry.path, size_bytes: entry.size_bytes }
}

fn text_response(result: Result<String, beskid_pckg_artifacts::ArtifactError>, content_type: &'static str) -> Response {
    match result {
        Ok(contents) => ([(header::CONTENT_TYPE, content_type)], contents).into_response(),
        Err(_) => hidden_not_found(),
    }
}

fn hidden_not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("package artifact not found"))).into_response()
}
