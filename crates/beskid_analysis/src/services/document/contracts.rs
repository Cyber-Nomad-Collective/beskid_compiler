use crate::resolve::ItemKind;

use super::model::{AnalysisSymbolKind, CompletionKind};

pub(super) fn analysis_symbol_kind_from_item_kind(kind: ItemKind) -> Option<AnalysisSymbolKind> {
    match kind {
        ItemKind::Function => Some(AnalysisSymbolKind::Function),
        ItemKind::Test => Some(AnalysisSymbolKind::Test),
        ItemKind::Method => Some(AnalysisSymbolKind::Method),
        ItemKind::Type => Some(AnalysisSymbolKind::Type),
        ItemKind::Enum => Some(AnalysisSymbolKind::Enum),
        ItemKind::Contract => Some(AnalysisSymbolKind::Contract),
        ItemKind::Module => Some(AnalysisSymbolKind::Module),
        ItemKind::Use => Some(AnalysisSymbolKind::Use),
        ItemKind::EnumVariant
        | ItemKind::Field
        | ItemKind::ContractNode
        | ItemKind::ContractMethodSignature
        | ItemKind::ContractEmbedding
        | ItemKind::Parameter
        | ItemKind::Statement => None,
    }
}

pub(super) fn completion_kind_from_item_kind(kind: ItemKind) -> CompletionKind {
    if let Some(symbol_kind) = analysis_symbol_kind_from_item_kind(kind) {
        return completion_kind_from_symbol_kind(symbol_kind);
    }

    match kind {
        ItemKind::EnumVariant => CompletionKind::EnumMember,
        ItemKind::Field => CompletionKind::Variable,
        ItemKind::ContractNode => CompletionKind::Method,
        ItemKind::ContractMethodSignature => CompletionKind::Method,
        ItemKind::ContractEmbedding => CompletionKind::Module,
        ItemKind::Parameter => CompletionKind::Variable,
        ItemKind::Statement => CompletionKind::Text,
        ItemKind::Function
        | ItemKind::Test
        | ItemKind::Method
        | ItemKind::Type
        | ItemKind::Enum
        | ItemKind::Contract
        | ItemKind::Module
        | ItemKind::Use => unreachable!("covered by analysis_symbol_kind_from_item_kind"),
    }
}

pub(super) fn completion_kind_from_symbol_kind(kind: AnalysisSymbolKind) -> CompletionKind {
    match kind {
        AnalysisSymbolKind::Function => CompletionKind::Function,
        AnalysisSymbolKind::Test => CompletionKind::Function,
        AnalysisSymbolKind::Method => CompletionKind::Method,
        AnalysisSymbolKind::Type => CompletionKind::Struct,
        AnalysisSymbolKind::Enum => CompletionKind::Enum,
        AnalysisSymbolKind::Contract => CompletionKind::Interface,
        AnalysisSymbolKind::Constant => CompletionKind::Variable,
        AnalysisSymbolKind::Module => CompletionKind::Module,
        AnalysisSymbolKind::Use => CompletionKind::Module,
    }
}

pub(super) fn item_kind_name(kind: ItemKind) -> &'static str {
    if let Some(symbol_kind) = analysis_symbol_kind_from_item_kind(kind) {
        return symbol_kind_name(symbol_kind);
    }

    match kind {
        ItemKind::EnumVariant => "enum variant",
        ItemKind::Field => "field",
        ItemKind::ContractNode => "contract node",
        ItemKind::ContractMethodSignature => "contract method",
        ItemKind::ContractEmbedding => "contract embedding",
        ItemKind::Parameter => "parameter",
        ItemKind::Statement => "statement",
        ItemKind::Function
        | ItemKind::Test
        | ItemKind::Method
        | ItemKind::Type
        | ItemKind::Enum
        | ItemKind::Contract
        | ItemKind::Module
        | ItemKind::Use => unreachable!("covered by analysis_symbol_kind_from_item_kind"),
    }
}

pub fn symbol_kind_name(kind: AnalysisSymbolKind) -> &'static str {
    match kind {
        AnalysisSymbolKind::Function => "function",
        AnalysisSymbolKind::Test => "test",
        AnalysisSymbolKind::Method => "method",
        AnalysisSymbolKind::Type => "type",
        AnalysisSymbolKind::Enum => "enum",
        AnalysisSymbolKind::Contract => "contract",
        AnalysisSymbolKind::Constant => "constant",
        AnalysisSymbolKind::Module => "module",
        AnalysisSymbolKind::Use => "use",
    }
}
