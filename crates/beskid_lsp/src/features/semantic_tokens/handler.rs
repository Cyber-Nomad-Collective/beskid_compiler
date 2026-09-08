use tower_lsp_server::ls_types::{SemanticTokens, SemanticTokensResult, Uri};

use crate::position::offset_to_position;
use crate::session::store::Document;
use crate::{
    features::semantic_tokens::encoder::{build_bsol_semantic_tokens, build_semantic_tokens},
    manifest_uri::{is_manifest_uri, is_standalone_bsol_uri},
};

/// Encoded declaration token stream from the document's current syntax generation.
pub fn handle_semantic_tokens(uri: &Uri, doc: &Document) -> SemanticTokensResult {
    let data = if is_manifest_uri(uri) || is_standalone_bsol_uri(uri) {
        build_bsol_semantic_tokens(&doc.text, offset_to_position)
    } else {
        build_semantic_tokens(&doc.text, &doc.syntax_symbols, offset_to_position)
    };
    SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data })
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
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: vec![SyntaxSymbol {
                name: "main".into(),
                kind: AnalysisSymbolKind::Function,
                start: 3,
                end: 7,
            }],
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        };

        let uri = "file:///main.bd".parse().expect("valid URI");
        let tokens = match handle_semantic_tokens(&uri, &doc) {
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
