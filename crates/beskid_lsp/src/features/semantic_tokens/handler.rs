use tower_lsp_server::ls_types::{SemanticTokens, SemanticTokensResult};

use crate::features::semantic_tokens::encoder::build_semantic_tokens;
use crate::position::offset_to_position;
use crate::session::store::Document;

/// Encoded declaration token stream from the document's current syntax generation.
pub fn handle_semantic_tokens(doc: &Document) -> SemanticTokensResult {
    SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: build_semantic_tokens(&doc.text, &doc.syntax_symbols, offset_to_position),
    })
}

#[cfg(test)]
mod tests {
    use beskid_analysis::services::AnalysisSymbolKind;

    use super::handle_semantic_tokens;
    use crate::session::store::{Document, SyntaxSymbol};

    #[test]
    fn syntax_tokens_work_without_legacy_analysis() {
        let doc = Document {
            version: 1,
            text: "fn main() {}".into(),
            analysis_cache_version: 0,
            analysis: None,
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: vec![SyntaxSymbol {
                name: "main".into(),
                kind: AnalysisSymbolKind::Function,
                start: 3,
                end: 7,
            }],
            syntax_completion: None,
        };

        let tokens = match handle_semantic_tokens(&doc) {
            tower_lsp_server::ls_types::SemanticTokensResult::Tokens(tokens) => tokens,
            tower_lsp_server::ls_types::SemanticTokensResult::Partial(_) => {
                panic!("full token handler cannot return a partial response")
            }
        };
        assert_eq!(tokens.data.len(), 1);
        assert_eq!(tokens.data[0].delta_start, 3);
        assert_eq!(tokens.data[0].length, 4);
        assert_eq!(tokens.data[0].token_type, 0);
        assert_eq!(tokens.data[0].token_modifiers_bitset, 1);
    }
}
