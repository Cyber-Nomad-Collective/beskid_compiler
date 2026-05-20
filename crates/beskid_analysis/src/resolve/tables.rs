//! Span-keyed resolution products and local symbol table used by type checking and codegen.

use std::collections::HashMap;

use crate::syntax::SpanInfo;

use super::ids::{ItemId, LocalId};

/// Result of resolving a value-position path or identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedValue {
    Item(ItemId),
    Local(LocalId),
}

/// Result of resolving a type-position path (named item or generic parameter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedType {
    Item(ItemId),
    Generic(String),
}

/// Name and span for a [`LocalId`] interned during resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInfo {
    pub id: LocalId,
    pub name: String,
    pub span: SpanInfo,
}

/// Maps expression/type spans to resolved symbols plus conformance edges.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ResolutionTables {
    pub resolved_values: HashMap<SpanInfo, ResolvedValue>,
    pub resolved_types: HashMap<SpanInfo, ResolvedType>,
    pub locals: Vec<LocalInfo>,
    pub type_conformances: HashMap<ItemId, Vec<(ItemId, SpanInfo)>>,
}

impl ResolutionTables {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_value(&mut self, span: SpanInfo, value: ResolvedValue) {
        self.resolved_values.insert(span, value);
    }

    pub fn insert_type(&mut self, span: SpanInfo, resolved_type: ResolvedType) {
        self.resolved_types.insert(span, resolved_type);
    }

    pub fn intern_local(&mut self, name: String, span: SpanInfo) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(LocalInfo { id, name, span });
        id
    }

    pub fn local_info(&self, id: LocalId) -> Option<&LocalInfo> {
        self.locals.get(id.0)
    }

    pub fn insert_type_conformance(
        &mut self,
        type_item_id: ItemId,
        contract_item_id: ItemId,
        span: SpanInfo,
    ) {
        let entries = self.type_conformances.entry(type_item_id).or_default();
        if entries
            .iter()
            .any(|(item_id, item_span)| *item_id == contract_item_id && *item_span == span)
        {
            return;
        }
        entries.push((contract_item_id, span));
    }

    /// Merge span-keyed products from `other` (later entries win on duplicate spans).
    pub fn merge_from(&mut self, other: &ResolutionTables) {
        self.resolved_types
            .extend(other.resolved_types.iter().map(|(k, v)| (*k, v.clone())));
        self.resolved_values
            .extend(other.resolved_values.iter().map(|(k, v)| (*k, *v)));
        for (type_id, edges) in &other.type_conformances {
            let dst = self.type_conformances.entry(*type_id).or_default();
            for edge in edges {
                if !dst.iter().any(|existing| existing == edge) {
                    dst.push(*edge);
                }
            }
        }
    }
}
