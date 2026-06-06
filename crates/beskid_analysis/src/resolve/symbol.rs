//! Package-prefixed canonical symbol identity for cross-unit lookup.

use std::collections::HashMap;

use super::ids::ItemId;
use super::items::ItemKind;

/// Fixed package name for compiler intrinsics ([`BuiltinSpec`](crate::builtins::BuiltinSpec)).
pub const BUILTIN_PACKAGE: &str = "beskid";

/// Interned key into [`SymbolRegistry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SymbolId(pub u32);

/// Package-prefixed canonical symbol identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolQualifier {
    pub package: String,
    pub shape: SymbolShape,
}

/// Structural shape of a symbol (how it encodes into a qualified name).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolShape {
    /// `package::module::path::Name`
    ModuleItem {
        module_path: Vec<String>,
        name: String,
        kind: ExportKind,
    },
    /// `package::parent_qn::member`
    Member {
        parent: SymbolId,
        name: String,
        kind: MemberKind,
    },
    /// `package::ReceiverType::method`
    Method { receiver: String, name: String },
    /// `beskid::path::segments`
    Builtin { path: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportKind {
    Function,
    Test,
    Type,
    Enum,
    Contract,
    Module,
    Use,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberKind {
    Parameter,
    Field,
    EnumVariant,
    ContractNode,
    ContractMethodSignature,
    ContractEmbedding,
    Statement,
}

impl ExportKind {
    pub const fn from_item_kind(kind: ItemKind) -> Option<Self> {
        match kind {
            ItemKind::Function => Some(Self::Function),
            ItemKind::Test => Some(Self::Test),
            ItemKind::Type => Some(Self::Type),
            ItemKind::Enum => Some(Self::Enum),
            ItemKind::Contract => Some(Self::Contract),
            ItemKind::Module => Some(Self::Module),
            ItemKind::Use => Some(Self::Use),
            _ => None,
        }
    }
}

impl MemberKind {
    pub const fn from_item_kind(kind: ItemKind) -> Option<Self> {
        match kind {
            ItemKind::Parameter => Some(Self::Parameter),
            ItemKind::Field => Some(Self::Field),
            ItemKind::EnumVariant => Some(Self::EnumVariant),
            ItemKind::ContractNode => Some(Self::ContractNode),
            ItemKind::ContractMethodSignature => Some(Self::ContractMethodSignature),
            ItemKind::ContractEmbedding => Some(Self::ContractEmbedding),
            ItemKind::Statement => Some(Self::Statement),
            _ => None,
        }
    }
}

/// Intern table for [`SymbolQualifier`] values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolRegistry {
    entries: Vec<SymbolQualifier>,
    lookup: HashMap<SymbolQualifier, SymbolId>,
}

impl SymbolRegistry {
    pub fn intern(&mut self, qualifier: SymbolQualifier) -> SymbolId {
        if let Some(id) = self.lookup.get(&qualifier) {
            return *id;
        }
        let id = SymbolId(self.entries.len() as u32);
        self.entries.push(qualifier.clone());
        self.lookup.insert(qualifier, id);
        id
    }

    pub fn lookup(&self, qualifier: &SymbolQualifier) -> Option<SymbolId> {
        self.lookup.get(qualifier).copied()
    }

    pub fn resolve(&self, id: SymbolId) -> Option<&SymbolQualifier> {
        self.entries.get(id.0 as usize)
    }

    pub fn entries(&self) -> &[SymbolQualifier] {
        &self.entries
    }

    pub fn merge_from(&mut self, other: &SymbolRegistry) {
        for qualifier in other.entries() {
            self.intern(qualifier.clone());
        }
    }
}

/// Canonical `::`-separated qualified name for docs, link plans, and LSP.
pub fn symbol_to_string(registry: &SymbolRegistry, qualifier: &SymbolQualifier) -> String {
    match &qualifier.shape {
        SymbolShape::ModuleItem {
            module_path, name, ..
        } => {
            if module_path.is_empty() {
                format!("{}::{}", qualifier.package, name)
            } else {
                format!(
                    "{}::{}::{}",
                    qualifier.package,
                    module_path.join("::"),
                    name
                )
            }
        }
        SymbolShape::Method { receiver, name } => {
            format!("{}::{}::{}", qualifier.package, receiver, name)
        }
        SymbolShape::Builtin { path } => {
            if path.is_empty() {
                qualifier.package.clone()
            } else {
                format!("{}::{}", qualifier.package, path.join("::"))
            }
        }
        SymbolShape::Member { parent, name, .. } => {
            let parent_q = registry
                .resolve(*parent)
                .map(|q| symbol_to_string(registry, q))
                .unwrap_or_else(|| qualifier.package.clone());
            format!("{parent_q}::{name}")
        }
    }
}

/// Stable string key for `api.json` (`symbolKey` field).
pub fn symbol_key(registry: &SymbolRegistry, id: SymbolId) -> Option<String> {
    registry.resolve(id).map(|q| symbol_to_string(registry, q))
}

/// Map a collected item to its symbol shape when exportable.
pub fn symbol_shape_for_item(
    kind: ItemKind,
    module_path: &[String],
    name: &str,
    method_receiver: Option<&str>,
    parent_symbol: Option<SymbolId>,
    member_display_name: Option<&str>,
) -> Option<SymbolShape> {
    if let Some(receiver) = method_receiver {
        if kind == ItemKind::Method {
            return Some(SymbolShape::Method {
                receiver: receiver.to_string(),
                name: name.rsplit("::").next().unwrap_or(name).to_string(),
            });
        }
    }
    if let (Some(parent), Some(member_name)) = (parent_symbol, member_display_name) {
        if let Some(member_kind) = MemberKind::from_item_kind(kind) {
            let short = member_name
                .splitn(2, "::")
                .nth(1)
                .unwrap_or(member_name)
                .to_string();
            return Some(SymbolShape::Member {
                parent,
                name: short,
                kind: member_kind,
            });
        }
    }
    if let Some(export_kind) = ExportKind::from_item_kind(kind) {
        return Some(SymbolShape::ModuleItem {
            module_path: module_path.to_vec(),
            name: name.to_string(),
            kind: export_kind,
        });
    }
    None
}

/// Register a symbol for an item; returns `None` when the item has no exportable symbol shape.
pub fn register_item_symbol(
    registry: &mut SymbolRegistry,
    by_symbol: &mut HashMap<SymbolId, ItemId>,
    package: &str,
    item_id: ItemId,
    shape: SymbolShape,
) -> SymbolId {
    let qualifier = SymbolQualifier {
        package: package.to_string(),
        shape,
    };
    let symbol_id = registry.intern(qualifier);
    by_symbol.insert(symbol_id, item_id);
    symbol_id
}
