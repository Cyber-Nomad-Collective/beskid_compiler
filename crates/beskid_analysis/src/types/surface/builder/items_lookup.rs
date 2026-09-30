use crate::paths;
use crate::resolve::{ItemId, ItemKind};
use crate::resolve::symbol::SymbolShape;
use crate::syntax::SpanInfo;

use super::state::TypeSurfaceBuilder;

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn visible_contract_in_owner_scope(&self, owner: ItemId, name: &str) -> Option<ItemId> {
        let mut current = self.declaring_module_id(owner);
        while let Some(module_id) = current {
            let module = self.resolution.module_graph.module(module_id)?;
            if let Some(&item) = module.scope.get(name) {
                return self.resolution.items.get(item.0).filter(|info| info.kind == ItemKind::Contract).map(|_| item);
            }
            current = module.parent;
        }
        None
    }

    fn declaring_module_id(&self, item: ItemId) -> Option<crate::resolve::ModuleId> {
        // Imported public items also appear in the importing module's item list. The
        // declaration symbol, unlike module membership, retains the lexical owner.
        let symbol = self.resolution.items.get(item.0)?.symbol?;
        let qualifier = self.resolution.symbols.resolve(symbol)?;
        let SymbolShape::ModuleItem { module_path, .. } = &qualifier.shape else {
            return None;
        };
        self.resolution.module_graph.module_id(module_path)
    }

    pub(super) fn contract_visible_to_owner(&self, owner: ItemId, contract: ItemId) -> bool {
        let Some(info) = self.resolution.items.get(contract.0) else {
            return false;
        };
        if info.kind != ItemKind::Contract {
            return false;
        }
        if info.visibility == crate::syntax::Visibility::Public {
            return true;
        }
        let Some(contract_module) = self.declaring_module_id(contract) else {
            return false;
        };
        let mut current = self.declaring_module_id(owner);
        while let Some(module_id) = current {
            if module_id == contract_module {
                return true;
            }
            current = self.resolution.module_graph.module(module_id).and_then(|module| module.parent);
        }
        false
    }

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
