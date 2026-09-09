use std::sync::{Arc, Mutex};

use beskid_pckg_store::PackageReviewRequest;

pub(super) const MAX_REVIEW_TEXT_BYTES: usize = 4000;

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
