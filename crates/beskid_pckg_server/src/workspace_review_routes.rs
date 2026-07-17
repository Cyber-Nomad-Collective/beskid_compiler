//! Workspace bundle publication and durable package review queue routes.
//!
//! Both adapters accept only verified Auth Hub session subjects. Workspace
//! publication owns every created member package, while review visibility and
//! actions use the same owner/moderator/delegated-moderator policy as the
//! administration surface.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{
    PackageArtifactStore, PublishRequest, ValidatedArtifact, validate_package_artifact,
};
use beskid_pckg_store::{
    AdminRole, AsyncAdministrationRepository, AsyncPackageReviewRepository, NewPackage,
    PackageReviewQueueError, PackageReviewRequest, PublishOutcome, StoreError,
    WorkspacePublishReservation,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{AppState, authenticated_subject, now_unix_seconds};

const MAX_WORKSPACE_BYTES: usize = 128 * 1024 * 1024;
const MAX_WORKSPACE_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_WORKSPACE_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REVIEW_TEXT_BYTES: usize = 4000;

#[derive(Clone, Copy)]
enum VersionBump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Default)]
pub(crate) struct ReviewQueueState {
    memory: Arc<Mutex<Vec<PackageReviewRequest>>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewSubmission {
    reason: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewAction {
    action: String,
    notes: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewResponse {
    id: String,
    package_id: String,
    package_name: String,
    requested_by_subject: String,
    reason: String,
    status: String,
    submitted_at_utc: String,
    reviewer_subject: Option<String>,
    review_notes: Option<String>,
    reviewed_at_utc: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePublishResponse {
    success: bool,
    message: String,
    workspace_name: Option<String>,
    packages: Vec<WorkspaceMemberResponse>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceMemberResponse {
    member_id: String,
    package_name: String,
    version: String,
    checksum_sha256: String,
    size_bytes: u64,
}

struct PreparedWorkspaceMember {
    member_id: String,
    package_name: String,
    package: NewPackage,
    version: String,
    artifact: Vec<u8>,
    validated: ValidatedArtifact,
}

pub(crate) async fn submit_review_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<ReviewSubmission>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let package = match state.packages.find_package(&name).await {
        Ok(Some(package)) => package,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    if package.owner_subject != subject {
        return not_found();
    }
    if !valid_review_text(&request.reason) {
        return bad_request("review reason must be non-empty and at most 4000 bytes");
    }
    let review = PackageReviewRequest {
        id: Uuid::new_v4().to_string(),
        package_id: package.id.clone(),
        requested_by_subject: subject,
        reason: request.reason.trim().to_owned(),
        status: "pending".to_owned(),
        submitted_at_unix_seconds: now_unix_seconds(),
        reviewer_subject: None,
        review_notes: None,
        reviewed_at_unix_seconds: None,
    };
    let saved = if let Some(repository) = &state.api_keys {
        match repository.submit_package_review(review).await {
            Ok(review) => review,
            Err(_) => return unavailable(),
        }
    } else {
        state
            .reviews
            .memory
            .lock()
            .expect("review queue mutex is not poisoned")
            .push(review.clone());
        review
    };
    (
        StatusCode::CREATED,
        Json(review_response(saved, package.name)),
    )
        .into_response()
}

pub(crate) async fn list_review_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let reviews = match all_reviews(&state).await {
        Ok(reviews) => reviews,
        Err(_) => return unavailable(),
    };
    let mut response = Vec::new();
    for review in reviews {
        let package = match state.packages.find_package_by_id(&review.package_id).await {
            Ok(Some(package)) => package,
            Ok(None) => continue,
            Err(_) => return unavailable(),
        };
        if can_moderate(&state, &subject, &package.owner_subject, &package.id).await {
            response.push(review_response(review, package.name));
        }
    }
    Json(response).into_response()
}

pub(crate) async fn review_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(review_id): Path<String>,
    Json(request): Json<ReviewAction>,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let Some(existing) = find_review(&state, &review_id).await else {
        return not_found();
    };
    let package = match state
        .packages
        .find_package_by_id(&existing.package_id)
        .await
    {
        Ok(Some(package)) => package,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    if !can_moderate(&state, &subject, &package.owner_subject, &package.id).await {
        return not_found();
    }
    let Some(status) = canonical_action(&request.action) else {
        return bad_request("action must be approved, needs_changes, or rejected");
    };
    let notes = request
        .notes
        .and_then(|notes| (!notes.trim().is_empty()).then(|| notes.trim().to_owned()));
    if notes
        .as_ref()
        .is_some_and(|notes| notes.len() > MAX_REVIEW_TEXT_BYTES)
    {
        return bad_request("review notes must be at most 4000 bytes");
    }
    let updated = if let Some(repository) = &state.api_keys {
        match repository
            .action_package_review(&review_id, status, &subject, notes, now_unix_seconds())
            .await
        {
            Ok(review) => review,
            Err(PackageReviewQueueError::NotFound) => return not_found(),
            Err(_) => return unavailable(),
        }
    } else {
        let mut reviews = state
            .reviews
            .memory
            .lock()
            .expect("review queue mutex is not poisoned");
        let Some(review) = reviews.iter_mut().find(|review| review.id == review_id) else {
            return not_found();
        };
        review.status = status.to_owned();
        review.reviewer_subject = Some(subject);
        review.review_notes = notes;
        review.reviewed_at_unix_seconds = Some(now_unix_seconds());
        review.clone()
    };
    Json(review_response(updated, package.name)).into_response()
}

pub(crate) async fn publish_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Response {
    let Some(subject) = authenticated_subject(&state, &headers) else {
        return crate::unauthorized_response();
    };
    let (bytes, version_bump) = match multipart_artifact(request).await {
        Ok(value) => value,
        Err(message) => return workspace_failure(StatusCode::BAD_REQUEST, message),
    };
    let workspace = match parse_workspace(&bytes) {
        Ok(workspace) => workspace,
        Err(message) => return workspace_failure(StatusCode::BAD_REQUEST, message),
    };
    let mut prepared = Vec::new();
    for member in &workspace.members {
        let existing = match state.packages.find_package(&member.package_name).await {
            Ok(Some(package)) if package.owner_subject == subject => Some(package),
            Ok(Some(_)) => {
                return workspace_failure(
                    StatusCode::FORBIDDEN,
                    "workspace member package is owned by another publisher",
                );
            }
            Ok(None) => None,
            Err(_) => {
                return workspace_failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "package persistence is unavailable",
                );
            }
        };
        let versions = match existing.as_ref() {
            Some(package) => match state.packages.list_versions(&package.id).await {
                Ok(versions) => versions,
                Err(_) => {
                    return workspace_failure(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "package persistence is unavailable",
                    );
                }
            },
            None => Vec::new(),
        };
        let version = next_version(
            versions.iter().map(|version| version.version.as_str()),
            version_bump,
        );
        let artifact = match build_member_artifact(&workspace, member, &version) {
            Ok(artifact) => artifact,
            Err(message) => return workspace_failure(StatusCode::BAD_REQUEST, message),
        };
        let validated = match validate_package_artifact(&artifact, &member.package_name, &version) {
            Ok(validated) => validated,
            Err(_) => {
                return workspace_failure(
                    StatusCode::BAD_REQUEST,
                    "workspace member artifact could not be validated",
                );
            }
        };
        prepared.push(PreparedWorkspaceMember {
            member_id: member.member_id.clone(),
            package_name: member.package_name.clone(),
            package: NewPackage {
                id: existing
                    .as_ref()
                    .map(|package| package.id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                name: member.package_name.clone(),
                owner_subject: subject.clone(),
                is_public: true,
                now_unix_seconds: now_unix_seconds(),
            },
            version,
            artifact,
            validated,
        });
    }
    // Stable lock order prevents two overlapping workspaces from deadlocking
    // their package rows in PostgreSQL.
    prepared.sort_by(|left, right| left.package_name.cmp(&right.package_name));
    let mut staged_new_keys: Vec<String> = Vec::new();
    let mut reservations = Vec::with_capacity(prepared.len());
    for member in &prepared {
        let staged = state.artifacts.save_staged(PublishRequest {
            validated: member.validated.clone(),
            bytes: &member.artifact,
        });
        let (stored, created) = match staged {
            Ok(stored) => stored,
            Err(_) => {
                for key in staged_new_keys {
                    let _ = state.artifacts.delete(&key);
                }
                return workspace_failure(
                    StatusCode::CONFLICT,
                    "workspace artifact conflicts with an immutable package version",
                );
            }
        };
        if created {
            staged_new_keys.push(stored.storage_key.clone());
        }
        reservations.push(WorkspacePublishReservation {
            package: member.package.clone(),
            version_id: Uuid::new_v4().to_string(),
            version: member.version.clone(),
            checksum_sha256: stored.checksum_sha256,
            storage_key: stored.storage_key,
            size_bytes: stored.size_bytes,
        });
    }
    let outcomes = match state.packages.publish_workspace_batch(reservations).await {
        Ok(outcomes) => outcomes,
        Err(StoreError::VersionImmutable) => {
            for key in staged_new_keys {
                let _ = state.artifacts.delete(&key);
            }
            return workspace_failure(
                StatusCode::CONFLICT,
                "workspace package version is immutable",
            );
        }
        Err(StoreError::PackageOwnershipConflict) => {
            for key in staged_new_keys {
                let _ = state.artifacts.delete(&key);
            }
            return workspace_failure(
                StatusCode::FORBIDDEN,
                "workspace member package is owned by another publisher",
            );
        }
        Err(_) => {
            for key in staged_new_keys {
                let _ = state.artifacts.delete(&key);
            }
            return workspace_failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "package persistence is unavailable",
            );
        }
    };
    let mut published = Vec::with_capacity(outcomes.len());
    for (member, outcome) in prepared.into_iter().zip(outcomes) {
        let version = match outcome.version {
            PublishOutcome::Created(version) | PublishOutcome::AlreadyExists(version) => version,
        };
        published.push(WorkspaceMemberResponse {
            member_id: member.member_id,
            package_name: member.package_name,
            version: version.version,
            checksum_sha256: version.checksum_sha256,
            size_bytes: version.size_bytes,
        });
    }
    Json(WorkspacePublishResponse {
        success: true,
        message: "Workspace packages published.".to_owned(),
        workspace_name: Some(workspace.name),
        packages: published,
    })
    .into_response()
}

async fn all_reviews(state: &AppState) -> Result<Vec<PackageReviewRequest>, ()> {
    if let Some(repository) = &state.api_keys {
        repository.list_package_reviews().await.map_err(|_| ())
    } else {
        Ok(state
            .reviews
            .memory
            .lock()
            .expect("review queue mutex is not poisoned")
            .clone())
    }
}

async fn find_review(state: &AppState, id: &str) -> Option<PackageReviewRequest> {
    all_reviews(state)
        .await
        .ok()?
        .into_iter()
        .find(|review| review.id == id)
}

async fn can_moderate(state: &AppState, subject: &str, owner: &str, package_id: &str) -> bool {
    if subject == owner {
        return true;
    }
    let Some(repository) = &state.api_keys else {
        return false;
    };
    if repository
        .roles_for_subject(subject)
        .await
        .map(|roles| {
            roles
                .iter()
                .any(|role| matches!(role, AdminRole::Moderator | AdminRole::SuperAdmin))
        })
        .unwrap_or(false)
    {
        return true;
    }
    repository
        .list_resource_permissions("package", package_id)
        .await
        .map(|grants| {
            grants
                .iter()
                .any(|grant| grant.subject == subject && grant.capability == "moderate")
        })
        .unwrap_or(false)
}

fn review_response(review: PackageReviewRequest, package_name: String) -> ReviewResponse {
    ReviewResponse {
        id: review.id,
        package_id: review.package_id,
        package_name,
        requested_by_subject: review.requested_by_subject,
        reason: review.reason,
        status: review.status,
        submitted_at_utc: rfc3339(review.submitted_at_unix_seconds),
        reviewer_subject: review.reviewer_subject,
        review_notes: review.review_notes,
        reviewed_at_utc: review.reviewed_at_unix_seconds.map(rfc3339),
    }
}

fn canonical_action(action: &str) -> Option<&'static str> {
    match action.trim().to_ascii_lowercase().as_str() {
        "approved" => Some("approved"),
        "needs_changes" | "needschanges" => Some("needs_changes"),
        "rejected" => Some("rejected"),
        _ => None,
    }
}
fn valid_review_text(value: &str) -> bool {
    !value.trim().is_empty() && value.trim() == value && value.len() <= MAX_REVIEW_TEXT_BYTES
}
fn rfc3339(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .expect("timestamp is valid")
        .to_rfc3339()
}
fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"message":"package review not found"})),
    )
        .into_response()
}
fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"message":"review persistence is unavailable"})),
    )
        .into_response()
}
fn bad_request(message: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"message":message})),
    )
        .into_response()
}
fn workspace_failure(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(WorkspacePublishResponse {
            success: false,
            message: message.into(),
            workspace_name: None,
            packages: Vec::new(),
        }),
    )
        .into_response()
}

async fn multipart_artifact(
    request: axum::extract::Request,
) -> Result<(Vec<u8>, VersionBump), &'static str> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or("Expected multipart form payload.")?;
    let boundary =
        multer::parse_boundary(content_type).map_err(|_| "Expected multipart form payload.")?;
    let constraints = multer::Constraints::new()
        .allowed_fields(vec!["artifact", "versionBump"])
        .size_limit(
            multer::SizeLimit::new()
                .whole_stream(MAX_WORKSPACE_BYTES as u64)
                .for_field("artifact", MAX_WORKSPACE_BYTES as u64),
        );
    let mut form = multer::Multipart::with_constraints(
        request.into_body().into_data_stream(),
        boundary,
        constraints,
    );
    let mut artifact = None;
    let mut version_bump = VersionBump::Patch;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| "Invalid workspace multipart payload.")?
    {
        let name = field.name().unwrap_or_default().to_owned();
        let bytes = field
            .bytes()
            .await
            .map_err(|_| "Invalid workspace multipart payload.")?;
        if name == "artifact" && artifact.is_none() {
            artifact = Some(bytes.to_vec());
        } else if name == "versionBump" {
            version_bump = match std::str::from_utf8(&bytes)
                .ok()
                .map(str::trim)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "" | "patch" => VersionBump::Patch,
                "minor" => VersionBump::Minor,
                "major" => VersionBump::Major,
                _ => return Err("versionBump must be patch, minor, or major."),
            };
        } else if name != "versionBump" {
            return Err("Invalid workspace multipart payload.");
        }
    }
    artifact
        .filter(|bytes| !bytes.is_empty())
        .map(|artifact| (artifact, version_bump))
        .ok_or("Artifact file is required.")
}

struct Workspace {
    name: String,
    entries: BTreeMap<String, Vec<u8>>,
    members: Vec<WorkspaceMember>,
}
struct WorkspaceMember {
    member_id: String,
    relative_path: String,
    package_name: String,
}

fn parse_workspace(bytes: &[u8]) -> Result<Workspace, &'static str> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "Workspace bundle is not a valid ZIP archive.")?;
    if !(1..=10_000).contains(&archive.len()) {
        return Err("Workspace bundle is empty or too large.");
    }
    let mut entries = BTreeMap::new();
    let mut uncompressed_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Workspace bundle is not a valid ZIP archive.")?;
        if entry.is_dir() {
            continue;
        }
        let entry_size = entry.size();
        if entry_size > MAX_WORKSPACE_ENTRY_BYTES {
            return Err("Workspace bundle contains an entry that exceeds the size limit.");
        }
        uncompressed_bytes = uncompressed_bytes.saturating_add(entry_size);
        if uncompressed_bytes > MAX_WORKSPACE_UNCOMPRESSED_BYTES {
            return Err("Workspace bundle exceeds the uncompressed size limit.");
        }
        let path = entry.name().replace('\\', "/");
        if path.is_empty()
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err("Workspace bundle contains an unsafe entry path.");
        }
        let mut contents = Vec::with_capacity(entry_size as usize);
        entry
            .read_to_end(&mut contents)
            .map_err(|_| "Workspace bundle could not be read.")?;
        if entries.insert(path, contents).is_some() {
            return Err("Workspace bundle contains duplicate entries.");
        }
    }
    let project = std::str::from_utf8(
        entries
            .get("Workspace.proj")
            .ok_or("Workspace bundle is missing 'Workspace.proj'.")?,
    )
    .map_err(|_| "Workspace.proj must be UTF-8.")?;
    let name =
        quoted_assignment(project, "name").ok_or("Workspace.proj is missing a workspace name.")?;
    let configured = entries
        .get("workspace.package.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let mut members = Vec::new();
    let lines = project.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(after) = trimmed.strip_prefix("member \"") else {
            continue;
        };
        let Some((member_id, after_id)) = after.split_once('"') else {
            return Err("Workspace.proj has an invalid member declaration.");
        };
        let mut member_block = after_id.to_owned();
        for candidate in lines.iter().skip(index + 1) {
            member_block.push('\n');
            member_block.push_str(candidate);
            if candidate.contains('}') {
                break;
            }
        }
        let relative_path = quoted_assignment(&member_block, "path")
            .ok_or("Workspace member is missing its path.")?;
        let package_name = configured
            .as_ref()
            .and_then(|value| {
                value
                    .get("members")?
                    .get(member_id)?
                    .get("package")?
                    .as_str()
            })
            .map(str::to_owned)
            .or_else(|| {
                entries
                    .get(&format!("{relative_path}/Project.proj"))
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(|manifest| quoted_assignment(manifest, "name"))
            })
            .ok_or("Workspace member is missing a package name.")?;
        if relative_path.contains("..") || package_name.trim().is_empty() {
            return Err("Workspace member is invalid.");
        }
        members.push(WorkspaceMember {
            member_id: member_id.to_owned(),
            relative_path,
            package_name,
        });
    }
    if members.is_empty() {
        return Err("Workspace bundle has no members.");
    }
    Ok(Workspace {
        name,
        entries,
        members,
    })
}

fn quoted_assignment(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        let rest = line
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?
            .trim();
        rest.strip_prefix('"')?
            .split_once('"')
            .map(|(value, _)| value.to_owned())
    })
}

fn build_member_artifact(
    workspace: &Workspace,
    member: &WorkspaceMember,
    version: &str,
) -> Result<Vec<u8>, &'static str> {
    let prefix = format!("{}/", member.relative_path);
    let mut entries = BTreeMap::new();
    for (path, bytes) in &workspace.entries {
        let Some(path) = path.strip_prefix(&prefix) else {
            continue;
        };
        if path == "Project.proj"
            || path.starts_with("src/")
            || path == "README.md"
            || path.starts_with("docs/")
            || path.starts_with(".beskid/docs/")
        {
            entries.insert(path.to_owned(), bytes.clone());
        }
    }
    if !entries.contains_key("Project.proj") || !entries.keys().any(|path| path.starts_with("src/"))
    {
        return Err("Workspace member must include Project.proj and source files.");
    }
    entries.insert("package.json".to_owned(), serde_json::to_vec(&serde_json::json!({"schema":"beskid.package.v1","id":member.package_name,"version":version,"packageKind":"library","dependencies":[]})).expect("JSON serialization succeeds"));
    let checksums = entries
        .iter()
        .map(|(path, bytes)| format!("{:x}  {path}", Sha256::digest(bytes)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    entries.insert("checksums.sha256".to_owned(), checksums.into_bytes());
    let mut output = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut output);
        for (path, bytes) in entries {
            zip.start_file(path, SimpleFileOptions::default())
                .map_err(|_| "Workspace artifact could not be created.")?;
            zip.write_all(&bytes)
                .map_err(|_| "Workspace artifact could not be created.")?;
        }
        zip.finish()
            .map_err(|_| "Workspace artifact could not be created.")?;
    }
    Ok(output.into_inner())
}

fn next_version<'a>(versions: impl Iterator<Item = &'a str>, bump: VersionBump) -> String {
    versions
        .filter_map(parse_version)
        .max()
        .map(|(major, minor, patch)| match bump {
            VersionBump::Patch => format!("{major}.{minor}.{}", patch + 1),
            VersionBump::Minor => format!("{major}.{}.0", minor + 1),
            VersionBump::Major => format!("{}.0.0", major + 1),
        })
        .unwrap_or_else(|| "0.0.1".to_owned())
}
fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}
