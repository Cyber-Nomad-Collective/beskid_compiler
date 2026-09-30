use std::collections::HashMap;
use std::path::PathBuf;

use super::model::{Registration, ScopeId};
use crate::syntax::SpanInfo;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompositionSnapshot {
    pub version: u32,
    pub launched_host: String,
    /// Canonical source unit that supplied the validated launch, bound by the frontend session.
    pub source_unit_path: Option<PathBuf>,
    pub launch_span: Option<SpanInfo>,
    pub registrations: Vec<Registration>,
    pub scope_names: HashMap<ScopeId, String>,
}
