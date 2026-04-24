use std::collections::HashMap;

use beskid_analysis::services::DocumentAnalysisSnapshot;
use tower_lsp_server::ls_types::Uri;

#[derive(Debug, Clone)]
pub struct Document {
    pub version: i32,
    pub text: String,
    pub text_hash: u64,
    pub analysis_cache_version: u32,
    pub analysis: Option<DocumentAnalysisSnapshot>,
}

#[derive(Default)]
pub struct State {
    pub docs: HashMap<Uri, Document>,
    /// Closed files on disk that still receive diagnostics (not managed by the editor buffer).
    pub workspace_index: HashMap<Uri, Document>,
}

impl State {
    pub fn document_union(&self, uri: &Uri) -> Option<Document> {
        self.docs.get(uri).cloned().or_else(|| self.workspace_index.get(uri).cloned())
    }
}
