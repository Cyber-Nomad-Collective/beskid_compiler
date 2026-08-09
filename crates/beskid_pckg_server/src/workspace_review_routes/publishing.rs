use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{PackageArtifactStore, PublishRequest, validate_package_artifact};
use beskid_pckg_store::{NewPackage, PublishOutcome, StoreError, WorkspacePublishReservation};
use uuid::Uuid;

use super::artifacts::build_member_artifact;
use super::contracts::{PreparedWorkspaceMember, WorkspaceMemberResponse, WorkspacePublishResponse};
use super::errors::workspace_failure;
use super::multipart::multipart_artifact;
use super::versions::next_version;
use super::workspace_parse::parse_workspace;
use crate::{AppState, authenticated_subject, now_unix_seconds};

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
                return workspace_failure(StatusCode::SERVICE_UNAVAILABLE, "package persistence is unavailable");
            }
        };
        let versions = match existing.as_ref() {
            Some(package) => match state.packages.list_versions(&package.id).await {
                Ok(versions) => versions,
                Err(_) => {
                    return workspace_failure(StatusCode::SERVICE_UNAVAILABLE, "package persistence is unavailable");
                }
            },
            None => Vec::new(),
        };
        let version = next_version(versions.iter().map(|version| version.version.as_str()), version_bump);
        let artifact = match build_member_artifact(&workspace, member, &version) {
            Ok(artifact) => artifact,
            Err(message) => return workspace_failure(StatusCode::BAD_REQUEST, message),
        };
        let validated = match validate_package_artifact(&artifact, &member.package_name, &version) {
            Ok(validated) => validated,
            Err(_) => {
                return workspace_failure(StatusCode::BAD_REQUEST, "workspace member artifact could not be validated");
            }
        };
        prepared.push(PreparedWorkspaceMember {
            member_id: member.member_id.clone(),
            package_name: member.package_name.clone(),
            package: NewPackage {
                id: existing.as_ref().map(|package| package.id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string()),
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
        let staged = state
            .artifacts
            .save_staged(PublishRequest { validated: member.validated.clone(), bytes: &member.artifact });
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
            return workspace_failure(StatusCode::CONFLICT, "workspace package version is immutable");
        }
        Err(StoreError::PackageOwnershipConflict) => {
            for key in staged_new_keys {
                let _ = state.artifacts.delete(&key);
            }
            return workspace_failure(StatusCode::FORBIDDEN, "workspace member package is owned by another publisher");
        }
        Err(_) => {
            for key in staged_new_keys {
                let _ = state.artifacts.delete(&key);
            }
            return workspace_failure(StatusCode::SERVICE_UNAVAILABLE, "package persistence is unavailable");
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
