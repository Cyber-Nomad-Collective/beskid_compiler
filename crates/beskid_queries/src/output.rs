//! Arc-backed query outputs for types that are not cheaply `Clone`.

use std::collections::HashMap;
use std::sync::Arc;

use beskid_analysis::resolve::{
    ItemId, Resolution, SymbolId, SymbolRegistry, canonical_item_id, item_id_for_symbol, qualified_name,
    symbol_for_item,
};
use beskid_analysis::services::ExecutableFrontEnd;

#[derive(Debug, Clone)]
pub struct SharedResolution(pub Arc<Resolution>);

impl SharedResolution {
    pub fn from_resolution(resolution: Resolution) -> Self {
        Self(Arc::new(resolution))
    }

    pub fn symbols(&self) -> &SymbolRegistry {
        &self.symbols
    }

    pub fn by_symbol(&self) -> &HashMap<SymbolId, ItemId> {
        &self.by_symbol
    }

    pub fn symbol_for_item(&self, item: ItemId) -> Option<SymbolId> {
        symbol_for_item(self, item)
    }

    pub fn item_id_for_symbol(&self, symbol: SymbolId) -> Option<ItemId> {
        item_id_for_symbol(self, symbol)
    }

    pub fn canonical_item_id(&self, item: ItemId) -> ItemId {
        canonical_item_id(self, item)
    }

    pub fn qualified_name(&self, item: ItemId) -> Option<String> {
        qualified_name(self, item)
    }
}

impl std::ops::Deref for SharedResolution {
    type Target = Resolution;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The executable front end of one prepared entry, shared between the entry session cache and
/// its consumers.
#[derive(Debug, Clone)]
pub struct SharedFrontEnd(pub Arc<ExecutableFrontEnd>);

impl std::ops::Deref for SharedFrontEnd {
    type Target = ExecutableFrontEnd;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
