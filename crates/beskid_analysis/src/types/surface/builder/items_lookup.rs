use crate::paths;
use crate::resolve::{ItemId, ItemKind};
use crate::syntax::SpanInfo;

use super::state::TypeSurfaceBuilder;

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        if let Some(info) = self.resolution.items.iter().find(|info| {
            info.span == span
                && info.source_path.as_ref().is_some_and(|source| paths::same_file(source, &self.source_path))
        }) {
            return Some(info.id);
        }
        let matches: Vec<_> = self.resolution.items.iter().filter(|info| info.span == span).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            _ => None,
        }
    }

    pub(super) fn item_id_for_name(&self, name: &str, kind: ItemKind) -> Option<ItemId> {
        let matches: Vec<_> =
            self.resolution.items.iter().filter(|info| info.name == name && info.kind == kind).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            many => many
                .iter()
                .rev()
                .find(|info| {
                    info.source_path.as_ref().is_some_and(|source| paths::same_file(source, &self.source_path))
                })
                .or_else(|| many.last())
                .map(|info| info.id),
        }
    }

    pub(super) fn canonical_item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        let item_id = self
            .resolution
            .items
            .iter()
            .find(|info| {
                info.span == span
                    && info.symbol.is_some()
                    && info.source_path.as_ref().is_some_and(|source| paths::same_file(source, &self.source_path))
            })
            .map(|info| info.id)
            .or_else(|| {
                let matches: Vec<_> =
                    self.resolution.items.iter().filter(|info| info.span == span && info.symbol.is_some()).collect();
                match matches.as_slice() {
                    [single] => Some(single.id),
                    _ => None,
                }
            })?;
        let symbol = self.resolution.items.get(item_id.0).and_then(|info| info.symbol)?;
        self.resolution.by_symbol.get(&symbol).copied().or(Some(item_id))
    }
}
