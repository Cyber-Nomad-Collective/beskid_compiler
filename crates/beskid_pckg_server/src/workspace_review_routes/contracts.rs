use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use beskid_pckg_artifacts::ValidatedArtifact;
use beskid_pckg_store::{NewPackage, PackageReviewRequest};

pub(super) const MAX_WORKSPACE_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_WORKSPACE_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_WORKSPACE_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_REVIEW_TEXT_BYTES: usize = 4000;

#[derive(Clone, Copy)]
pub(super) enum VersionBump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Default)]
pub(crate) struct ReviewQueueState {
    pub(super) memory: Arc<Mutex<Vec<PackageReviewRequest>>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewSubmission {
    pub(super) reason: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewAction {
    pub(super) action: String,
    pub(super) notes: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReviewResponse {
    pub(super) id: String,
    pub(super) package_id: String,
    pub(super) package_name: String,
    pub(super) requested_by_subject: String,
    pub(super) reason: String,
    pub(super) status: String,
    pub(super) submitted_at_utc: String,
    pub(super) reviewer_subject: Option<String>,
    pub(super) review_notes: Option<String>,
    pub(super) reviewed_at_utc: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorkspacePublishResponse {
    pub(super) success: bool,
    pub(super) message: String,
    pub(super) workspace_name: Option<String>,
    pub(super) packages: Vec<WorkspaceMemberResponse>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorkspaceMemberResponse {
    pub(super) member_id: String,
    pub(super) package_name: String,
    pub(super) version: String,
    pub(super) checksum_sha256: String,
    pub(super) size_bytes: u64,
}

pub(super) struct PreparedWorkspaceMember {
    pub(super) member_id: String,
    pub(super) package_name: String,
    pub(super) package: NewPackage,
    pub(super) version: String,
    pub(super) artifact: Vec<u8>,
    pub(super) validated: ValidatedArtifact,
}

pub(super) struct Workspace {
    pub(super) name: String,
    pub(super) entries: BTreeMap<String, Vec<u8>>,
    pub(super) members: Vec<WorkspaceMember>,
}

pub(super) struct WorkspaceMember {
    pub(super) member_id: String,
    pub(super) relative_path: String,
    pub(super) package_name: String,
}
