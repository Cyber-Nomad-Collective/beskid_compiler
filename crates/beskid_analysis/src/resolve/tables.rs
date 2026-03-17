use std::collections::HashMap;

use crate::syntax::SpanInfo;

use super::ids::{ItemId, LocalId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedValue {
    Item(ItemId),
    Local(LocalId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedType {
    Item(ItemId),
    Generic(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInfo {
    pub id: LocalId,
    pub name: String,
    pub span: SpanInfo,
}

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
}
