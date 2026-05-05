use crate::hir::HirVisibility;
use crate::syntax::SpanInfo;

use super::ids::ItemId;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: ItemId,
    pub name: String,
    pub kind: ItemKind,
    pub visibility: HirVisibility,
    pub span: SpanInfo,
}
