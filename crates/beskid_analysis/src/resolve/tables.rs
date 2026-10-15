//! Span-keyed resolution products and local symbol table used by type checking and codegen.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::paths::same_file_opt;
use crate::syntax::SpanInfo;

use super::ids::{ItemId, LocalId};

fn value_at_span<T: Copy>(values: &HashMap<SpanInfo, T>, span: SpanInfo) -> Option<T> {
    values.get(&span).copied()
}

enum ScopedLookup<T> {
    Missing,
    Found(T),
    Ambiguous,
}
fn scoped_at<T: Clone + PartialEq>(
    maps: &HashMap<PathBuf, HashMap<SpanInfo, T>>,
    path: &PathBuf,
    span: SpanInfo,
) -> ScopedLookup<T> {
    // Exact source identity outranks the materialized-source suffix correspondence.
    if let Some(values) = maps.get(path) {
        return values.get(&span).cloned().map_or(ScopedLookup::Missing, ScopedLookup::Found);
    }
    let identity = crate::paths::ScopedPathIdentity::new(path);
    if let Some(values) = identity.canonical().and_then(|canonical| maps.get(canonical)) {
        return values.get(&span).cloned().map_or(ScopedLookup::Missing, ScopedLookup::Found);
    }
    let mut candidate = None;
    for (scoped_path, values) in maps {
        // Most units do not contain this span. Do not touch their filesystem paths.
        let Some(value) = values.get(&span) else { continue };
        if !identity.matches(scoped_path) {
            continue;
        }
        if candidate.as_ref().is_some_and(|existing| existing != value) {
            return ScopedLookup::Ambiguous;
        }
        candidate = Some(value.clone());
    }
    candidate.map_or(ScopedLookup::Missing, ScopedLookup::Found)
}

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
    /// Literal facts, not callable items or storage slots. Source identity prevents span collisions.
    pub integer_constants: HashMap<(Option<PathBuf>, SpanInfo), crate::syntax::Spanned<crate::syntax::Literal>>,
    pub resolved_values: HashMap<SpanInfo, ResolvedValue>,
    pub resolved_types: HashMap<SpanInfo, ResolvedType>,
    /// `use` path spans that supplied a symbol which resolved successfully.
    ///
    /// This is resolution provenance rather than a source-spelling approximation.
    pub used_import_spans: HashSet<SpanInfo>,
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

    pub fn resolved_value_at(&self, span: SpanInfo, source_path: Option<&PathBuf>) -> Option<ResolvedValue> {
        if let Some(path) = source_path {
            return match scoped_at(&self.scoped_resolved_values, path, span) {
                ScopedLookup::Found(value) => Some(value),
                ScopedLookup::Ambiguous => None,
                ScopedLookup::Missing => value_at_span(&self.resolved_values, span),
            };
        }
        let mut candidate = None;
        for values in self.scoped_resolved_values.values() {
            let Some(value) = value_at_span(values, span) else { continue };
            if candidate.is_some_and(|existing| existing != value) {
                return None;
            }
            candidate = Some(value);
        }
        candidate.or_else(|| value_at_span(&self.resolved_values, span))
    }

    pub fn insert_type(&mut self, span: SpanInfo, resolved_type: ResolvedType) {
        self.resolved_types.insert(span, resolved_type);
    }

    /// Dependency surfaces consume only source-scoped facts, never entry-unit fallback.
    pub(crate) fn scoped_resolved_type_at(&self, span: SpanInfo, source_path: &PathBuf) -> Option<ResolvedType> {
        match scoped_at(&self.scoped_resolved_types, source_path, span) {
            ScopedLookup::Found(value) => Some(value),
            ScopedLookup::Missing | ScopedLookup::Ambiguous => None,
        }
    }

    pub fn resolved_type_at(&self, span: SpanInfo, source_path: Option<&PathBuf>) -> Option<ResolvedType> {
        if let Some(path) = source_path {
            return match scoped_at(&self.scoped_resolved_types, path, span) {
                ScopedLookup::Found(value) => Some(value),
                ScopedLookup::Ambiguous => None,
                ScopedLookup::Missing => self.resolved_types.get(&span).cloned(),
            };
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

    pub fn intern_local(&mut self, name: String, span: SpanInfo, source_path: Option<PathBuf>) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(LocalInfo { id, name, span, source_path });
        id
    }

    pub fn local_id_for_span(&self, span: SpanInfo, source_path: Option<&PathBuf>) -> Option<LocalId> {
        if let Some(id) = self
            .locals
            .iter()
            .find(|info| info.span == span && same_file_opt(info.source_path.as_ref(), source_path))
            .map(|info| info.id)
        {
            return Some(id);
        }

        if let Some(path) = source_path {
            let matches: Vec<&LocalInfo> = self
                .locals
                .iter()
                .filter(|info| info.span.start == span.start && same_file_opt(info.source_path.as_ref(), Some(path)))
                .collect();
            if matches.len() == 1 {
                return Some(matches[0].id);
            }
        }

        let matches: Vec<&LocalInfo> = self.locals.iter().filter(|info| info.span == span).collect();
        if matches.len() == 1 {
            return Some(matches[0].id);
        }

        let start_matches: Vec<&LocalInfo> = self.locals.iter().filter(|info| info.span.start == span.start).collect();
        if start_matches.len() == 1 {
            return Some(start_matches[0].id);
        }
        None
    }

    pub fn local_info(&self, id: LocalId) -> Option<&LocalInfo> {
        self.locals.get(id.0)
    }

    pub fn insert_type_conformance(&mut self, type_item_id: ItemId, contract_item_id: ItemId, span: SpanInfo) {
        let entries = self.type_conformances.entry(type_item_id).or_default();
        if entries.iter().any(|(item_id, item_span)| *item_id == contract_item_id && *item_span == span) {
            return;
        }
        entries.push((contract_item_id, span));
    }

    /// Merge span-keyed products from `other`, remapping [`LocalId`] values from dependency units.
    pub fn merge_from(&mut self, other: &ResolutionTables, unit_source_path: PathBuf) {
        self.integer_constants.extend(other.integer_constants.clone());
        let mut local_remap: HashMap<LocalId, LocalId> = HashMap::new();
        for local in &other.locals {
            if let Some(existing) = self.local_id_for_span(local.span, local.source_path.as_ref()) {
                local_remap.insert(local.id, existing);
                continue;
            }
            let new_id = self.intern_local(local.name.clone(), local.span, local.source_path.clone());
            local_remap.insert(local.id, new_id);
        }

        let scoped_values =
            self.scoped_resolved_values.entry(crate::paths::unit_path_key(&unit_source_path)).or_default();
        for (span, value) in &other.resolved_values {
            let remapped = match value {
                ResolvedValue::Local(id) => ResolvedValue::Local(*local_remap.get(id).unwrap_or(id)),
                other => *other,
            };
            scoped_values.insert(*span, remapped);
        }

        self.merge_declaration_types_from(other, unit_source_path);
    }

    /// Merge only declaration type facts and conformance edges for one dependency unit.
    pub(crate) fn merge_declaration_types_from(&mut self, other: &ResolutionTables, unit_source_path: PathBuf) {
        let scoped_types =
            self.scoped_resolved_types.entry(crate::paths::unit_path_key(&unit_source_path)).or_default();
        scoped_types.extend(other.resolved_types.iter().map(|(k, v)| (*k, v.clone())));

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

#[cfg(test)]
mod scoped_lookup_tests {
    use super::*;
    fn span() -> SpanInfo {
        SpanInfo { start: 1, end: 4, line_col_start: (1, 2), line_col_end: (1, 5) }
    }
    fn unit(value: &str) -> ResolutionTables {
        let mut table = ResolutionTables::new();
        table.insert_type(span(), ResolvedType::Generic(value.into()));
        table
    }
    #[test]
    fn v06_scoped_lookup_exact_existing_sources_do_not_alias_by_suffix() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("one/src/Same.bd");
        let b = root.path().join("two/src/Same.bd");
        for path in [&a, &b] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "type Same {}").unwrap();
        }
        let mut table = ResolutionTables::new();
        table.merge_from(&unit("A"), a.clone());
        table.merge_from(&unit("B"), b.clone());
        assert_eq!(table.resolved_type_at(span(), Some(&a)), Some(ResolvedType::Generic("A".into())));
        assert_eq!(table.resolved_type_at(span(), Some(&b)), Some(ResolvedType::Generic("B".into())));
    }
    #[test]
    fn v06_scoped_lookup_missing_materialized_alias_conflicts_fail_closed() {
        let mut table = ResolutionTables::new();
        table.merge_from(&unit("A"), PathBuf::from("missing-a/src/Same.bd"));
        table.merge_from(&unit("B"), PathBuf::from("missing-b/src/Same.bd"));
        assert_eq!(table.resolved_type_at(span(), Some(&PathBuf::from("missing-query/src/Same.bd"))), None);
    }
    #[test]
    fn v06_scoped_lookup_missing_exact_unit_and_new_tables_preserve_scope() {
        let path = PathBuf::from("missing/Distinct.bd");
        let mut old = ResolutionTables::new();
        old.merge_from(&unit("old"), path.clone());
        let mut new = ResolutionTables::new();
        new.merge_from(&unit("new"), path.clone());
        assert_eq!(old.resolved_type_at(span(), Some(&path)), Some(ResolvedType::Generic("old".into())));
        assert_eq!(new.resolved_type_at(span(), Some(&path)), Some(ResolvedType::Generic("new".into())));
    }
    #[test]
    fn v06_scoped_lookup_dependency_surface_never_uses_entry_fallback() {
        let mut table = ResolutionTables::new();
        table.insert_type(span(), ResolvedType::Generic("entryOnly".into()));
        let dependency = PathBuf::from("missing-dependency/src/Other.bd");
        assert_eq!(table.resolved_type_at(span(), Some(&dependency)), Some(ResolvedType::Generic("entryOnly".into())));
        assert_eq!(table.scoped_resolved_type_at(span(), &dependency), None);
        table.merge_from(&unit("dependency"), dependency.clone());
        assert_eq!(table.scoped_resolved_type_at(span(), &dependency), Some(ResolvedType::Generic("dependency".into())));
    }

    #[test]
    #[cfg(unix)]
    fn v06_scoped_lookup_symlink_retarget_uses_current_file_identity() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("A.bd");
        let b = root.path().join("B.bd");
        let alias = root.path().join("Alias.bd");
        std::fs::write(&a, "type A {}").unwrap();
        std::fs::write(&b, "type B {}").unwrap();
        std::os::unix::fs::symlink(&a, &alias).unwrap();
        let mut table = ResolutionTables::new();
        table.merge_from(&unit("A"), a);
        table.merge_from(&unit("B"), b.clone());
        assert_eq!(table.resolved_type_at(span(), Some(&alias)), Some(ResolvedType::Generic("A".into())));
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&b, &alias).unwrap();
        assert_eq!(table.resolved_type_at(span(), Some(&alias)), Some(ResolvedType::Generic("B".into())));
    }
}
