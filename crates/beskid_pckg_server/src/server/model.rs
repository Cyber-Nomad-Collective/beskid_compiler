use std::sync::Arc;

use beskid_pckg_artifacts::LocalFileArtifactStore;
use beskid_pckg_store::SqlxPackageRepository;

use super::backend_memory::PackageBackend;
use super::config::AuthConfig;
use crate::{community_routes, operations_routes, workspace_review_routes};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) auth: Option<AuthConfig>,
    pub(crate) packages: PackageBackend,
    pub(crate) artifacts: Arc<LocalFileArtifactStore>,
    pub(crate) api_keys: Option<Arc<SqlxPackageRepository>>,
    pub(crate) community: community_routes::CommunityState,
    pub(crate) reviews: workspace_review_routes::ReviewQueueState,
    pub(crate) operations: operations_routes::OperationsState,
}
