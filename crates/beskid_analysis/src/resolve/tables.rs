//! Span-keyed resolution products and local symbol table used by type checking and codegen.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::paths::same_file_opt;
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
    pub source_path: Option<PathBuf>,
}

/// Maps expression/type spans to resolved symbols plus conformance edges.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ResolutionTables {
    pub resolved_values: HashMap<SpanInfo, ResolvedValue>,
    pub resolved_types: HashMap<SpanInfo, ResolvedType>,
    /// Per-unit value resolutions merged from dependency compilation units.
    pub scoped_resolved_values: HashMap<PathBuf, HashMap<SpanInfo, ResolvedValue>>,
    /// Per-unit type resolutions merged from dependency compilation units.
    pub scoped_resolved_types: HashMap<PathBuf, HashMap<SpanInfo, ResolvedType>>,
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

    pub fn resolved_value_at(
        &self,
        span: SpanInfo,
        source_path: Option<&PathBuf>,
    ) -> Option<ResolvedValue> {
        if let Some(path) = source_path
            && let Some(value) = self.scoped_value_at(path, span)
        {
            return Some(value);
        }

        let mut candidate: Option<ResolvedValue> = None;
        for values in self.scoped_resolved_values.values() {
            if let Some(value) = values.get(&span) {
                if candidate.is_some() {
                    candidate = None;
                    break;
                }
                candidate = Some(*value);
            }
        }
        if let Some(value) = candidate {
            return Some(value);
        }

        let flat = self.resolved_values.get(&span).copied();
        flat
    }

    fn scoped_value_at(&self, path: &PathBuf, span: SpanInfo) -> Option<ResolvedValue> {
        for (scoped_path, values) in &self.scoped_resolved_values {
            if same_file_opt(Some(scoped_path), Some(path)) {
                if let Some(value) = values.get(&span) {
                    return Some(*value);
                }
            }
        }
        None
    }

    pub fn insert_type(&mut self, span: SpanInfo, resolved_type: ResolvedType) {
        self.resolved_types.insert(span, resolved_type);
    }

    pub fn resolved_type_at(
        &self,
        span: SpanInfo,
        source_path: Option<&PathBuf>,
    ) -> Option<ResolvedType> {
        if let Some(path) = source_path
            && let Some(resolved_type) = self.scoped_type_at(path, span)
        {
            return Some(resolved_type);
        }

        let mut candidate: Option<ResolvedType> = None;
        for types in self.scoped_resolved_types.values() {
            if let Some(resolved_type) = types.get(&span) {
                if candidate.is_some() {
                    candidate = None;
                    break;
                }
                candidate = Some(resolved_type.clone());
            }
        }
        if let Some(resolved_type) = candidate {
            return Some(resolved_type);
        }

        self.resolved_types.get(&span).cloned()
    }

    fn scoped_type_at(&self, path: &PathBuf, span: SpanInfo) -> Option<ResolvedType> {
        for (scoped_path, types) in &self.scoped_resolved_types {
            if same_file_opt(Some(scoped_path), Some(path)) {
                if let Some(resolved_type) = types.get(&span) {
                    return Some(resolved_type.clone());
                }
            }
        }
        None
    }

    pub fn intern_local(
        &mut self,
        name: String,
        span: SpanInfo,
        source_path: Option<PathBuf>,
    ) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(LocalInfo {
            id,
            name,
            span,
            source_path,
        });
        id
    }

    pub fn local_id_for_span(
        &self,
        span: SpanInfo,
        source_path: Option<&PathBuf>,
    ) -> Option<LocalId> {
        if let Some(id) = self
            .locals
            .iter()
            .find(|info| info.span == span && same_file_opt(info.source_path.as_ref(), source_path))
            .map(|info| info.id)
        {
            return Some(id);
        }

        let matches: Vec<&LocalInfo> = self.locals.iter().filter(|info| info.span == span).collect();
        if matches.len() == 1 {
            return Some(matches[0].id);
        }
        None
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

    /// Merge span-keyed products from `other`, remapping [`LocalId`] values from dependency units.
    pub fn merge_from(&mut self, other: &ResolutionTables, unit_source_path: PathBuf) {
        let mut local_remap: HashMap<LocalId, LocalId> = HashMap::new();
        for local in &other.locals {
            if let Some(existing) = self.local_id_for_span(local.span, local.source_path.as_ref()) {
                local_remap.insert(local.id, existing);
                continue;
            }
            let new_id = self.intern_local(
                local.name.clone(),
                local.span,
                local.source_path.clone(),
            );
            local_remap.insert(local.id, new_id);
        }

        let scoped_types = self
            .scoped_resolved_types
            .entry(
                unit_source_path
                    .canonicalize()
                    .unwrap_or_else(|_| unit_source_path.clone()),
            )
            .or_default();
        scoped_types.extend(
            other
                .resolved_types
                .iter()
                .map(|(k, v)| (*k, v.clone())),
        );

        let scoped_values = self
            .scoped_resolved_values
            .entry(
                unit_source_path
                    .canonicalize()
                    .unwrap_or(unit_source_path),
            )
            .or_default();
        for (span, value) in &other.resolved_values {
            let remapped = match value {
                ResolvedValue::Local(id) => {
                    ResolvedValue::Local(*local_remap.get(id).unwrap_or(id))
                }
                other => *other,
            };
            scoped_values.insert(*span, remapped);
        }

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

