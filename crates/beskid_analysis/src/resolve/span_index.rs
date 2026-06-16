//! Sorted span index for exact resolution lookup (replaces start-offset fuzzy fallback).

use crate::syntax::SpanInfo;

use super::tables::{ResolvedType, ResolvedValue};

#[derive(Debug, Clone, PartialEq, Eq)]
enum SpanTarget {
    Value(ResolvedValue),
    Type(ResolvedType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpanEntry {
    start: usize,
    end: usize,
    target: SpanTarget,
}

/// Per-resolution sorted `(start, end, kind, target)` entries for binary-search lookup.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SpanIndex {
    entries: Vec<SpanEntry>,
}

impl SpanIndex {
    pub fn build_from_maps(
        values: &[(SpanInfo, ResolvedValue)],
        types: &[(SpanInfo, ResolvedType)],
    ) -> Self {
        let mut entries = Vec::with_capacity(values.len() + types.len());
        for (span, value) in values {
            entries.push(SpanEntry {
                start: span.start,
                end: span.end,
                target: SpanTarget::Value(*value),
            });
        }
        for (span, resolved_type) in types {
            entries.push(SpanEntry {
                start: span.start,
                end: span.end,
                target: SpanTarget::Type(resolved_type.clone()),
            });
        }
        entries.sort_by_key(|entry| (entry.start, entry.end));
        Self { entries }
    }

    pub fn lookup_value(&self, span: SpanInfo) -> Option<ResolvedValue> {
        self.lookup(span).and_then(|target| match target {
            SpanTarget::Value(value) => Some(value),
            SpanTarget::Type(_) => None,
        })
    }

    pub fn lookup_type(&self, span: SpanInfo) -> Option<ResolvedType> {
        self.lookup(span).and_then(|target| match target {
            SpanTarget::Type(resolved_type) => Some(resolved_type),
            SpanTarget::Value(_) => None,
        })
    }

    fn lookup(&self, span: SpanInfo) -> Option<SpanTarget> {
        if let Some(exact) = self.exact_match(span) {
            return Some(exact);
        }
        self.innermost_containing(span)
    }

    fn exact_match(&self, span: SpanInfo) -> Option<SpanTarget> {
        self.entries
            .binary_search_by_key(&(span.start, span.end), |entry| (entry.start, entry.end))
            .ok()
            .map(|index| self.entries[index].target.clone())
    }

    fn innermost_containing(&self, span: SpanInfo) -> Option<SpanTarget> {
        let mut best: Option<(usize, SpanTarget)> = None;
        for entry in &self.entries {
            if entry.start > span.start {
                break;
            }
            if entry.start <= span.start && entry.end >= span.end {
                let size = entry.end.saturating_sub(entry.start);
                match best {
                    Some((best_size, _)) if size >= best_size => {}
                    _ => best = Some((size, entry.target.clone())),
                }
            }
        }
        best.map(|(_, target)| target)
    }
}

/// Build a [`SpanIndex`] from merged resolution tables.
pub fn span_index_from_tables(tables: &super::tables::ResolutionTables) -> SpanIndex {
    let mut values = Vec::new();
    for (span, value) in &tables.resolved_values {
        values.push((*span, *value));
    }
    for map in tables.scoped_resolved_values.values() {
        for (span, value) in map {
            values.push((*span, *value));
        }
    }
    let mut types = Vec::new();
    for (span, resolved_type) in &tables.resolved_types {
        types.push((*span, resolved_type.clone()));
    }
    for map in tables.scoped_resolved_types.values() {
        for (span, resolved_type) in map {
            types.push((*span, resolved_type.clone()));
        }
    }
    SpanIndex::build_from_maps(&values, &types)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::ids::{ItemId, LocalId};

    #[test]
    fn exact_span_lookup_wins_over_outer() {
        let outer = SpanInfo {
            start: 0,
            end: 20,
            ..Default::default()
        };
        let inner = SpanInfo {
            start: 5,
            end: 10,
            ..Default::default()
        };
        let index = SpanIndex::build_from_maps(
            &[
                (outer, ResolvedValue::Item(ItemId(1))),
                (inner, ResolvedValue::Local(LocalId(2))),
            ],
            &[],
        );
        assert_eq!(
            index.lookup_value(inner),
            Some(ResolvedValue::Local(LocalId(2)))
        );
    }

    #[test]
    fn same_start_prefers_exact_end() {
        let a = SpanInfo {
            start: 10,
            end: 15,
            ..Default::default()
        };
        let b = SpanInfo {
            start: 10,
            end: 20,
            ..Default::default()
        };
        let index = SpanIndex::build_from_maps(
            &[
                (a, ResolvedValue::Item(ItemId(1))),
                (b, ResolvedValue::Item(ItemId(2))),
            ],
            &[],
        );
        assert_eq!(index.lookup_value(a), Some(ResolvedValue::Item(ItemId(1))));
    }
}
