//! Source-authoritative event operation facts.

use super::facts::ItemSignature;
use super::ids::AstNodeKey;
use std::sync::Arc;

/// One checked operation on a declared event field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EventOperationKind {
    Subscribe,
    UnsubscribeFirst,
    Raise,
}

/// Generation-bound event operation authority consumed by syntax-to-ISLE adaptation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EventOperationFact {
    pub operation: EventOperationKind,
    pub operation_node: AstNodeKey,
    pub receiver: AstNodeKey,
    pub declaration: AstNodeKey,
    pub field: AstNodeKey,
    pub slot_offset: u32,
    pub capacity: u32,
    pub handler: Option<AstNodeKey>,
    pub arguments: Arc<[AstNodeKey]>,
    pub delegate_signature: Option<ItemSignature>,
}
