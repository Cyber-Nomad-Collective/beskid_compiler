use beskid_analysis::services::AnalysisSymbolKind;
use tower_lsp_server::ls_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

const TOKEN_TYPE_FUNCTION: u32 = 0;
const TOKEN_TYPE_METHOD: u32 = 1;
const TOKEN_TYPE_STRUCT: u32 = 2;
const TOKEN_TYPE_ENUM: u32 = 3;
const TOKEN_TYPE_INTERFACE: u32 = 4;
const TOKEN_TYPE_NAMESPACE: u32 = 5;

const TOKEN_MODIFIER_DECLARATION: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct SemanticTokenCandidate {
    start: usize,
    end: usize,
    token_type: u32,
    token_modifiers_bitset: u32,
    priority: u8,
}

pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::FUNCTION,
            SemanticTokenType::METHOD,
            SemanticTokenType::STRUCT,
            SemanticTokenType::ENUM,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::NAMESPACE,
        ],
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

fn push_semantic_symbol_tokens(
    symbols: &[crate::session::store::SyntaxSymbol],
    out: &mut Vec<SemanticTokenCandidate>,
) {
    for symbol in symbols {
        let token_type = match symbol.kind {
            AnalysisSymbolKind::Function => TOKEN_TYPE_FUNCTION,
            AnalysisSymbolKind::Test => TOKEN_TYPE_FUNCTION,
            AnalysisSymbolKind::Method => TOKEN_TYPE_METHOD,
            AnalysisSymbolKind::Type => TOKEN_TYPE_STRUCT,
            AnalysisSymbolKind::Enum => TOKEN_TYPE_ENUM,
            AnalysisSymbolKind::Contract => TOKEN_TYPE_INTERFACE,
            AnalysisSymbolKind::Module | AnalysisSymbolKind::Use => TOKEN_TYPE_NAMESPACE,
        };

        out.push(SemanticTokenCandidate {
            start: symbol.start,
            end: symbol.end,
            token_type,
            token_modifiers_bitset: TOKEN_MODIFIER_DECLARATION,
            priority: 10,
        });
    }
}

/// Build the declaration-only token stream from the document's current syntax generation.
///
/// `SyntaxSymbol` is constructed from the exact `SyntaxIndex` snapshot held by the session;
/// semantic tokens must not reach into the optional legacy HIR analysis snapshot.
pub fn build_semantic_tokens(
    text: &str,
    symbols: &[crate::session::store::SyntaxSymbol],
    offset_to_position: impl Fn(&str, usize) -> tower_lsp_server::ls_types::Position,
) -> Vec<SemanticToken> {
    let mut candidates = Vec::new();
    push_semantic_symbol_tokens(symbols, &mut candidates);

    candidates.sort_by_key(|candidate| (candidate.start, candidate.end, candidate.priority));

    let mut merged: Vec<SemanticTokenCandidate> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if let Some(last) = merged.last_mut()
            && last.start == candidate.start
            && last.end == candidate.end
        {
            if candidate.priority >= last.priority {
                *last = candidate;
            }
            continue;
        }
        merged.push(candidate);
    }

    let mut tokens = Vec::with_capacity(merged.len());
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for candidate in merged {
        if candidate.end <= candidate.start || candidate.end > text.len() {
            continue;
        }

        let start = offset_to_position(text, candidate.start);
        let end = offset_to_position(text, candidate.end);
        if start.line != end.line || end.character <= start.character {
            continue;
        }

        let delta_line = start.line.saturating_sub(prev_line);
        let delta_start = if delta_line == 0 {
            start.character.saturating_sub(prev_char)
        } else {
            start.character
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: end.character.saturating_sub(start.character),
            token_type: candidate.token_type,
            token_modifiers_bitset: candidate.token_modifiers_bitset,
        });

        prev_line = start.line;
        prev_char = start.character;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use beskid_analysis::services::AnalysisSymbolKind;

    use super::build_semantic_tokens;
    use crate::position::offset_to_position;
    use crate::session::store::SyntaxSymbol;

    #[test]
    fn syntax_symbols_preserve_legend_order_and_delta_encoding() {
        let text = "fn first() {}\nstruct Second {}";
        let tokens = build_semantic_tokens(
            text,
            &[
                SyntaxSymbol {
                    name: "first".into(),
                    kind: AnalysisSymbolKind::Function,
                    start: 3,
                    end: 8,
                },
                SyntaxSymbol {
                    name: "Second".into(),
                    kind: AnalysisSymbolKind::Type,
                    start: 21,
                    end: 27,
                },
            ],
            offset_to_position,
        );

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].delta_line, 0);
        assert_eq!(tokens[0].delta_start, 3);
        assert_eq!(tokens[0].length, 5);
        assert_eq!(tokens[0].token_type, 0); // FUNCTION is the first legend entry.
        assert_eq!(tokens[0].token_modifiers_bitset, 1); // DECLARATION is bit zero.
        assert_eq!(tokens[1].delta_line, 1);
        assert_eq!(tokens[1].delta_start, 7);
        assert_eq!(tokens[1].length, 6);
        assert_eq!(tokens[1].token_type, 2); // STRUCT is the third legend entry.
        assert_eq!(tokens[1].token_modifiers_bitset, 1);
    }
}
