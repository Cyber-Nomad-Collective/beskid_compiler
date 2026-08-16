use std::path::PathBuf;

use crate::syntax::SpanInfo;
use crate::syntax::Visibility;

use super::ids::ItemId;
use super::symbol::SymbolId;

/// Classification of each [`ItemInfo`] row (used for docs, queries, and stable API snapshots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Function,
    Test,
    Method,
    Type,
    Enum,
    EnumVariant,
    Field,
    Contract,
    ContractNode,
    ContractMethodSignature,
    ContractEmbedding,
    Parameter,
    Statement,
    Module,
    Use,
}

impl ItemKind {
    /// Stable snake-case identifier for machine-readable API docs (`api.json`).
    pub const fn as_stable_doc_kind(self) -> &'static str {
        match self {
            ItemKind::Function => "function",
            ItemKind::Test => "test",
            ItemKind::Method => "method",
            ItemKind::Type => "type",
            ItemKind::Enum => "enum",
            ItemKind::EnumVariant => "enum_variant",
            ItemKind::Field => "field",
            ItemKind::Contract => "contract",
            ItemKind::ContractNode => "contract_node",
            ItemKind::ContractMethodSignature => "contract_method",
            ItemKind::ContractEmbedding => "contract_embedding",
            ItemKind::Parameter => "parameter",
            ItemKind::Statement => "statement",
            ItemKind::Module => "module",
            ItemKind::Use => "use",
        }
    }
}

/// One resolved declaration: stable id, stable name, kind, visibility, and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: ItemId,
    /// Immediate lexical parent item when this row was collected as a member (e.g. type fields).
    pub parent_id: Option<ItemId>,
    pub name: String,
    pub kind: ItemKind,
    pub visibility: Visibility,
    pub span: SpanInfo,
    /// Declaring source file when known (assembly prefetch or entry unit).
    pub source_path: Option<PathBuf>,
    /// Canonical package-prefixed symbol when this row is exportable.
    pub symbol: Option<SymbolId>,
}
