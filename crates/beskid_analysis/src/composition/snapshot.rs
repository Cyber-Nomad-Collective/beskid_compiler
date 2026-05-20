use std::collections::HashMap;

use super::model::{Registration, ScopeId};
use crate::syntax::SpanInfo;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompositionSnapshot {
    pub version: u32,
    pub launched_host: String,
    pub launch_span: Option<SpanInfo>,
    pub registrations: Vec<Registration>,
    pub scope_names: HashMap<ScopeId, String>,
}
