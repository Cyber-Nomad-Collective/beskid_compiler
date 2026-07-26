use beskid_analysis::services::AnalysisSymbolKind;
use tower_lsp_server::ls_types::SymbolKind;

pub fn analysis_symbol_kind_to_lsp(kind: AnalysisSymbolKind) -> SymbolKind {
    match kind {
        AnalysisSymbolKind::Function => SymbolKind::FUNCTION,
        AnalysisSymbolKind::Test => SymbolKind::FUNCTION,
        AnalysisSymbolKind::Method => SymbolKind::METHOD,
        AnalysisSymbolKind::Type => SymbolKind::STRUCT,
        AnalysisSymbolKind::Enum => SymbolKind::ENUM,
        AnalysisSymbolKind::Contract => SymbolKind::INTERFACE,
        AnalysisSymbolKind::Constant => SymbolKind::CONSTANT,
        AnalysisSymbolKind::Module => SymbolKind::MODULE,
        AnalysisSymbolKind::Use => SymbolKind::NAMESPACE,
    }
}

#[cfg(test)]
mod tests {
    use super::analysis_symbol_kind_to_lsp;
    use beskid_analysis::services::AnalysisSymbolKind;
    use tower_lsp_server::ls_types::SymbolKind;

    #[test]
    fn constant_symbols_use_the_lsp_constant_kind() {
        assert_eq!(analysis_symbol_kind_to_lsp(AnalysisSymbolKind::Constant), SymbolKind::CONSTANT);
    }
}
