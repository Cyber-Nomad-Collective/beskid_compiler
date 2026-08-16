use std::path::Path;

use crate::projects::assembly::ProgramAssembly;
use crate::resolve::{ItemId, LocalId, Resolution, ResolvedValue, SymbolId, canonical_item_id, symbol_for_item};

use super::contracts::item_kind_name;
use super::model::{DefinitionInfo, DocumentAnalysisSnapshot, HoverInfo, ReferenceInfo, SymbolLocation};

fn symbol_location_for_item(item: &crate::resolve::ItemInfo, fallback_path: &Path) -> SymbolLocation {
    SymbolLocation {
        path: item.source_path.clone().unwrap_or_else(|| fallback_path.to_path_buf()),
        start: item.span.start,
        end: item.span.end,
    }
}

fn symbol_location_for_span(path: &Path, start: usize, end: usize) -> SymbolLocation {
    SymbolLocation { path: path.to_path_buf(), start, end }
}

fn resolved_value_at_offset(resolution: &Resolution, offset: usize) -> Option<&ResolvedValue> {
    resolution
        .tables
        .resolved_values
        .iter()
        .filter(|(span, _)| span.start <= offset && offset <= span.end)
        .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
        .map(|(_, resolved)| resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceTarget {
    Local(LocalId),
    Symbol(SymbolId),
    Item(ItemId),
}

fn reference_target(resolution: &Resolution, resolved: &ResolvedValue) -> ReferenceTarget {
    match resolved {
        ResolvedValue::Local(local_id) => ReferenceTarget::Local(*local_id),
        ResolvedValue::Item(item_id) => {
            if let Some(symbol) = symbol_for_item(resolution, *item_id) {
                ReferenceTarget::Symbol(symbol)
            } else {
                ReferenceTarget::Item(*item_id)
            }
        }
    }
}

fn reference_targets_match(
    entry_resolution: &Resolution,
    target: ReferenceTarget,
    unit_resolution: &Resolution,
    candidate: &ResolvedValue,
) -> bool {
    match (target, candidate) {
        (ReferenceTarget::Local(target_local), ResolvedValue::Local(candidate_local)) => {
            target_local == *candidate_local
        }
        (ReferenceTarget::Symbol(target_symbol), ResolvedValue::Item(candidate_item)) => {
            symbol_for_item(unit_resolution, *candidate_item) == Some(target_symbol)
        }
        (ReferenceTarget::Item(target_item), ResolvedValue::Item(candidate_item)) => target_item == *candidate_item,
        (ReferenceTarget::Symbol(target_symbol), _) => {
            reference_target(entry_resolution, candidate) == ReferenceTarget::Symbol(target_symbol)
        }
        _ => false,
    }
}

/// Resolved item id at `offset` for documentation routing (definitions and enclosing items).
pub fn item_id_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<ItemId> {
    let resolution = snapshot.resolution.as_ref()?;
    if let Some(resolved) = resolved_value_at_offset(resolution, offset) {
        return match resolved {
            ResolvedValue::Item(item_id) => Some(*item_id),
            ResolvedValue::Local(_) => None,
        };
    }
    resolution
        .items
        .iter()
        .filter(|item| item.span.start <= offset && offset <= item.span.end)
        .min_by_key(|item| item.span.end.saturating_sub(item.span.start))
        .map(|item| item.id)
}

pub fn hover_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<HoverInfo> {
    let resolution = snapshot.resolution.as_ref()?;
    if let Some(resolved) = resolved_value_at_offset(resolution, offset) {
        return match resolved {
            ResolvedValue::Item(item_id) => hover_for_item(snapshot, item_id.0),
            ResolvedValue::Local(local_id) => {
                let local = resolution.tables.local_info(*local_id)?;
                Some(HoverInfo {
                    markdown: format!("**local** `{}`", local.name),
                    location: symbol_location_for_span(&snapshot.source_path, local.span.start, local.span.end),
                })
            }
        };
    }
    resolution
        .items
        .iter()
        .filter(|item| item.span.start <= offset && offset <= item.span.end)
        .min_by_key(|item| item.span.end.saturating_sub(item.span.start))
        .and_then(|item| hover_for_item(snapshot, item.id.0))
}

fn hover_for_item(snapshot: &DocumentAnalysisSnapshot, item_idx: usize) -> Option<HoverInfo> {
    let item = snapshot.resolution.as_ref()?.items.get(item_idx)?;
    let mut markdown = format!("**{}** `{}`", item_kind_name(item.kind), item.name);
    if let Some(doc) = snapshot.item_docs.get(item_idx).and_then(|slot| slot.as_ref())
        && !doc.markdown.trim().is_empty()
    {
        markdown.push_str("\n\n---\n\n");
        markdown.push_str(&doc.markdown);
    }
    Some(HoverInfo { markdown, location: symbol_location_for_item(item, &snapshot.source_path) })
}

pub fn definition_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<DefinitionInfo> {
    let resolution = snapshot.resolution.as_ref()?;
    let resolved = resolved_value_at_offset(resolution, offset)?;
    match resolved {
        ResolvedValue::Item(item_id) => {
            let item_id = canonical_item_id(resolution, *item_id);
            let item = resolution.items.get(item_id.0)?;
            Some(DefinitionInfo { location: symbol_location_for_item(item, &snapshot.source_path) })
        }
        ResolvedValue::Local(local_id) => {
            let local = resolution.tables.local_info(*local_id)?;
            Some(DefinitionInfo {
                location: symbol_location_for_span(&snapshot.source_path, local.span.start, local.span.end),
            })
        }
    }
}

pub fn references_at_offset(
    snapshot: &DocumentAnalysisSnapshot,
    offset: usize,
    include_declaration: bool,
) -> Vec<ReferenceInfo> {
    let Some(resolution) = snapshot.resolution.as_ref() else {
        return Vec::new();
    };

    let Some(target_resolved) = resolved_value_at_offset(resolution, offset).copied() else {
        return Vec::new();
    };
    let target = reference_target(resolution, &target_resolved);

    let mut references: Vec<ReferenceInfo> = resolution
        .tables
        .resolved_values
        .iter()
        .filter_map(|(span, resolved)| {
            if reference_targets_match(resolution, target, resolution, resolved) {
                Some(ReferenceInfo { location: symbol_location_for_span(&snapshot.source_path, span.start, span.end) })
            } else {
                None
            }
        })
        .collect();

    if include_declaration {
        match target_resolved {
            ResolvedValue::Item(item_id) => {
                let item_id = canonical_item_id(resolution, item_id);
                if let Some(item) = resolution.items.get(item_id.0) {
                    references.push(ReferenceInfo { location: symbol_location_for_item(item, &snapshot.source_path) });
                }
            }
            ResolvedValue::Local(local_id) => {
                if let Some(local) = resolution.tables.local_info(local_id) {
                    references.push(ReferenceInfo {
                        location: symbol_location_for_span(&snapshot.source_path, local.span.start, local.span.end),
                    });
                }
            }
        }
    }

    references
        .sort_by_key(|reference| (reference.location.path.clone(), reference.location.start, reference.location.end));
    references.dedup_by(|left, right| left.location == right.location);
    references
}

pub fn references_at_offset_workspace(
    snapshot: &DocumentAnalysisSnapshot,
    assembly: &ProgramAssembly,
    entry_path: &Path,
    offset: usize,
    include_declaration: bool,
) -> Vec<ReferenceInfo> {
    let mut references = references_at_offset(snapshot, offset, include_declaration);

    let resolution = match snapshot.resolution.as_ref() {
        Some(r) => r,
        None => return references,
    };
    let Some(target_resolved) = resolved_value_at_offset(resolution, offset).copied() else {
        return references;
    };
    let target = reference_target(resolution, &target_resolved);

    for (index, unit_program) in assembly.units.iter().enumerate() {
        if index == assembly.entry_index {
            continue;
        }
        let Ok(unit_resolution) = assembly.module_index.resolve_unit_program(&unit_program.program, &unit_program.path)
        else {
            continue;
        };
        for (span, resolved) in &unit_resolution.tables.resolved_values {
            if !reference_targets_match(resolution, target, &unit_resolution, resolved) {
                continue;
            }
            references
                .push(ReferenceInfo { location: symbol_location_for_span(&unit_program.path, span.start, span.end) });
        }
    }

    if include_declaration
        && let ResolvedValue::Item(item_id) = target_resolved
        && let item_id = canonical_item_id(resolution, item_id)
        && let Some(item) = resolution.items.get(item_id.0)
        && item.source_path.as_ref().is_some_and(|path| path != entry_path)
    {
        references.push(ReferenceInfo { location: symbol_location_for_item(item, entry_path) });
    }

    references
        .sort_by_key(|reference| (reference.location.path.clone(), reference.location.start, reference.location.end));
    references.dedup_by(|left, right| left.location == right.location);
    references
}

#[cfg(test)]
mod reference_target_tests {
    use std::collections::HashMap;

    use crate::resolve::{
        ExportKind, ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution, ResolutionTables, ResolvedValue, SymbolId,
        SymbolQualifier, SymbolRegistry, SymbolShape,
    };
    use crate::syntax::SpanInfo;
    use crate::syntax::Visibility;

    use super::{ReferenceTarget, reference_target, reference_targets_match};

    fn span(start: usize, end: usize) -> SpanInfo {
        SpanInfo::from_byte_range_in_source("", start, end)
    }

    fn item_with_symbol(id: usize, symbol: SymbolId) -> ItemInfo {
        ItemInfo {
            id: ItemId(id),
            parent_id: None,
            name: "SharedFn".into(),
            kind: ItemKind::Function,
            visibility: Visibility::Public,
            span: span(0, 8),
            source_path: None,
            symbol: Some(symbol),
        }
    }

    #[test]
    fn reference_targets_match_same_symbol_different_item_ids() {
        let mut registry = SymbolRegistry::default();
        let symbol = registry.intern(SymbolQualifier {
            package: "demo".into(),
            shape: SymbolShape::ModuleItem {
                module_path: vec!["Root".into()],
                name: "SharedFn".into(),
                kind: ExportKind::Function,
            },
        });

        let entry_item_id = ItemId(0);
        let unit_item_id = ItemId(1);
        let entry_resolution = Resolution {
            items: vec![item_with_symbol(0, symbol)],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            span_index: Default::default(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: registry.clone(),
            by_symbol: HashMap::from([(symbol, entry_item_id)]),
        };
        let unit_resolution = Resolution {
            items: vec![
                ItemInfo {
                    id: ItemId(0),
                    parent_id: None,
                    name: "Other".into(),
                    kind: ItemKind::Function,
                    visibility: Visibility::Public,
                    span: span(0, 4),
                    source_path: None,
                    symbol: None,
                },
                item_with_symbol(1, symbol),
            ],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            span_index: Default::default(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: registry,
            by_symbol: HashMap::from([(symbol, unit_item_id)]),
        };

        let target = reference_target(&entry_resolution, &ResolvedValue::Item(entry_item_id));
        assert_eq!(target, ReferenceTarget::Symbol(symbol));

        assert!(
            reference_targets_match(&entry_resolution, target, &unit_resolution, &ResolvedValue::Item(unit_item_id),),
            "same SymbolId must match across units even when ItemId differs"
        );
        assert!(!reference_targets_match(
            &entry_resolution,
            target,
            &unit_resolution,
            &ResolvedValue::Item(ItemId(99)),
        ));
    }
}
