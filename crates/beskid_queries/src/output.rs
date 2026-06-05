//! Arc-backed query outputs for types that are not cheaply `Clone`.

use std::collections::HashMap;
use std::sync::Arc;

use beskid_analysis::resolve::{
    canonical_item_id, item_id_for_symbol, qualified_name, symbol_for_item, ItemId, Resolution,
    SymbolId, SymbolRegistry,
};
use beskid_analysis::services::FrontEndTypedResult;
use beskid_analysis::types::TypeResult;

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

#[derive(Debug, Clone)]
pub struct SharedTypeResult(pub Arc<TypeResult>);

impl std::ops::Deref for SharedTypeResult {
    type Target = TypeResult;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct SharedFrontEnd(pub Arc<FrontEndTypedResult>);

impl std::ops::Deref for SharedFrontEnd {
    type Target = FrontEndTypedResult;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
