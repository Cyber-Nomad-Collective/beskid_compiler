//! Resolve [`ItemId`] ↔ [`SymbolId`] via [`Resolution`].

use super::ids::ItemId;
use super::resolver::Resolution;
use super::symbol::{SymbolId, SymbolQualifier, SymbolRegistry, symbol_key, symbol_to_string};

pub fn symbol_for_item(resolution: &Resolution, item: ItemId) -> Option<SymbolId> {
    resolution.items.get(item.0).and_then(|info| info.symbol)
}

pub fn item_id_for_symbol(resolution: &Resolution, symbol: SymbolId) -> Option<ItemId> {
    resolution.by_symbol.get(&symbol).copied()
}

/// Prefer the authoritative [`ItemId`] from [`Resolution::by_symbol`] when present.
pub fn canonical_item_id(resolution: &Resolution, item: ItemId) -> ItemId {
    symbol_for_item(resolution, item)
        .and_then(|symbol| item_id_for_symbol(resolution, symbol))
        .unwrap_or(item)
}

pub fn item_id_for_qualifier(
    resolution: &Resolution,
    qualifier: &SymbolQualifier,
) -> Option<ItemId> {
    resolution
        .symbols
        .lookup(qualifier)
        .and_then(|symbol| item_id_for_symbol(resolution, symbol))
}

pub fn qualified_name(resolution: &Resolution, item: ItemId) -> Option<String> {
    let symbol = symbol_for_item(resolution, item)?;
    symbol_key(&resolution.symbols, symbol)
}

pub fn qualified_name_from_symbol(registry: &SymbolRegistry, symbol: SymbolId) -> Option<String> {
    symbol_key(registry, symbol)
}

pub fn symbol_string(registry: &SymbolRegistry, qualifier: &SymbolQualifier) -> String {
    symbol_to_string(registry, qualifier)
}
