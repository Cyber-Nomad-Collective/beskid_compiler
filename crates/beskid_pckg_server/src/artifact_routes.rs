//! Read-only, visibility-preserving views over verified package artifacts.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{
    ArtifactBrowser, ArtifactFileContent, ArtifactRecord, BrowseEntry, PackageArtifactStore, ValidatedArtifact,
    parse_package_manifest_metadata, select_download,
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

/// README plus the package's generated `api.json` (schema v4 graph model)
/// when the publisher packed one.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredDocumentationResponse {
    readme: Option<String>,
    api: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileListResponse {
    package: String,
    version: String,
    entries: Vec<BrowseEntryResponse>,
}

/// One artifact file for display. `encoding` tells the client how to treat
/// `content`: `utf-8` carries the text, `binary` and `too-large` carry `null`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileContentResponse {
    path: String,
    size_bytes: u64,
    encoding: &'static str,
    limit_bytes: Option<u64>,
    content: Option<String>,
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
            Json(StructuredDocumentationResponse { readme: documentation.readme, api: documentation.api })
                .into_response()
        }
        Err(_) => hidden_not_found(),
    }
}

/// Every displayable file of the verified artifact, in archive path order.
pub(crate) async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
) -> Response {
    let Some((browser, version)) = browser_and_version_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    Json(FileListResponse {
        package: path.name,
        version,
        entries: browser.list_files().into_iter().map(entry_response).collect(),
    })
    .into_response()
}

/// One displayable file. Path traversal, hidden entries and unknown entries
/// all answer with the same not-found so the archive layout never leaks.
pub(crate) async fn read_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<ArtifactPath>,
    Query(query): Query<BrowseFileQuery>,
) -> Response {
    let Some(browser) = browser_for_request(&state, &headers, &path).await else {
        return hidden_not_found();
    };
    let Some(requested) = query.path.as_deref() else {
        return hidden_not_found();
    };
    match browser.read_file(requested) {
        Ok(file) => {
            let (encoding, limit_bytes, content) = match file.content {
                ArtifactFileContent::Text(text) => ("utf-8", None, Some(text)),
                ArtifactFileContent::Binary => ("binary", None, None),
                ArtifactFileContent::TooLarge { limit_bytes } => ("too-large", Some(limit_bytes), None),
            };
            Json(FileContentResponse { path: file.path, size_bytes: file.size_bytes, encoding, limit_bytes, content })
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
    browser_and_version_for_request(state, headers, path).await.map(|(browser, _)| browser)
}

/// Resolves the browser together with the concrete version `latest` mapped to.
async fn browser_and_version_for_request(
    state: &AppState,
    headers: &HeaderMap,
    path: &ArtifactPath,
) -> Option<(ArtifactBrowser, String)> {
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
    let metadata = parse_package_manifest_metadata(&version.manifest_json, &package.name, &version.version).ok()?;
    let resolved_version = version.version.clone();
    let browser = ArtifactBrowser::from_validated_bytes(
        &bytes,
        &ValidatedArtifact {
            package_name: package.name,
            version: version.version,
            checksum_sha256: version.checksum_sha256,
            size_bytes: version.size_bytes,
            manifest_json: version.manifest_json,
            metadata,
        },
    )
    .ok()?;
    Some((browser, resolved_version))
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
