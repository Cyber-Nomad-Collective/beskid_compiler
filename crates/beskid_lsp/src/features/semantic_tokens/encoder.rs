use beskid_analysis::services::AnalysisSymbolKind;
use tower_lsp_server::ls_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};

use crate::session::store::{BsolSemanticTokenCandidate, BsolSemanticTokenKind, SyntaxAttributeName};

const TOKEN_TYPE_FUNCTION: u32 = 0;
const TOKEN_TYPE_METHOD: u32 = 1;
const TOKEN_TYPE_STRUCT: u32 = 2;
const TOKEN_TYPE_ENUM: u32 = 3;
const TOKEN_TYPE_INTERFACE: u32 = 4;
const TOKEN_TYPE_NAMESPACE: u32 = 5;
const TOKEN_TYPE_VARIABLE: u32 = 6;
const TOKEN_TYPE_PROPERTY: u32 = 7;
/// Attribute names at every declaration site and declared attribute names.
pub(crate) const TOKEN_TYPE_DECORATOR: u32 = 8;

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
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::DECORATOR,
        ],
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

fn push_semantic_symbol_tokens(symbols: &[crate::session::store::SyntaxSymbol], out: &mut Vec<SemanticTokenCandidate>) {
    for symbol in symbols {
        let token_type = match symbol.kind {
            AnalysisSymbolKind::Function => TOKEN_TYPE_FUNCTION,
            AnalysisSymbolKind::Test => TOKEN_TYPE_FUNCTION,
            AnalysisSymbolKind::Method => TOKEN_TYPE_METHOD,
            AnalysisSymbolKind::Type => TOKEN_TYPE_STRUCT,
            AnalysisSymbolKind::Enum => TOKEN_TYPE_ENUM,
            AnalysisSymbolKind::Contract => TOKEN_TYPE_INTERFACE,
            AnalysisSymbolKind::Constant => TOKEN_TYPE_VARIABLE,
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

fn push_attribute_name_tokens(names: &[SyntaxAttributeName], out: &mut Vec<SemanticTokenCandidate>) {
    for name in names {
        out.push(SemanticTokenCandidate {
            start: name.start,
            end: name.end,
            token_type: TOKEN_TYPE_DECORATOR,
            token_modifiers_bitset: if name.declaration { TOKEN_MODIFIER_DECLARATION } else { 0 },
            priority: 15,
        });
    }
}

fn encode_semantic_tokens(
    text: &str,
    mut candidates: Vec<SemanticTokenCandidate>,
    offset_to_position: impl Fn(&str, usize) -> tower_lsp_server::ls_types::Position,
) -> Vec<SemanticToken> {
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
        let delta_start = if delta_line == 0 { start.character.saturating_sub(prev_char) } else { start.character };

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

/// Build the declaration-only token stream from the document's current syntax generation.
///
/// `SyntaxSymbol` is constructed from the exact `SyntaxIndex` snapshot held by the session;
/// semantic tokens must not reach into the optional legacy HIR analysis snapshot.
pub fn build_semantic_tokens(
    text: &str,
    symbols: &[crate::session::store::SyntaxSymbol],
    attribute_names: &[SyntaxAttributeName],
    offset_to_position: impl Fn(&str, usize) -> tower_lsp_server::ls_types::Position,
) -> Vec<SemanticToken> {
    let mut candidates = Vec::new();
    push_semantic_symbol_tokens(symbols, &mut candidates);
    push_attribute_name_tokens(attribute_names, &mut candidates);
    encode_semantic_tokens(text, candidates, offset_to_position)
}

/// Encode structural declaration tokens from the document generation's BSOL facts.
///
/// Invalid BSOL has an empty candidate snapshot: syntax highlighting remains the Tree-sitter
/// layer's responsibility, while the LSP never publishes partial semantic facts.
pub fn build_bsol_semantic_tokens(
    text: &str,
    bsol_candidates: &[BsolSemanticTokenCandidate],
    offset_to_position: impl Fn(&str, usize) -> tower_lsp_server::ls_types::Position,
) -> Vec<SemanticToken> {
    let candidates = bsol_candidates
        .iter()
        .map(|candidate| SemanticTokenCandidate {
            start: candidate.start,
            end: candidate.end,
            token_type: match candidate.kind {
                BsolSemanticTokenKind::Namespace => TOKEN_TYPE_NAMESPACE,
                BsolSemanticTokenKind::Property => TOKEN_TYPE_PROPERTY,
            },
            token_modifiers_bitset: TOKEN_MODIFIER_DECLARATION,
            priority: 20,
        })
        .collect();
    encode_semantic_tokens(text, candidates, offset_to_position)
}

#[cfg(test)]
mod tests {
    use beskid_analysis::services::AnalysisSymbolKind;

    use super::{build_bsol_semantic_tokens, build_semantic_tokens, semantic_token_legend};
    use crate::position::offset_to_position;
    use crate::session::store::{BsolSemanticTokenCandidate, BsolSemanticTokenKind, SyntaxSymbol};

    #[test]
    fn semantic_legend_exposes_standard_property_tokens_for_bsol_keys() {
        assert!(
            semantic_token_legend().token_types.contains(&tower_lsp_server::ls_types::SemanticTokenType::PROPERTY),
            "BSOL assignment keys need the standard property token type"
        );
    }

    #[test]
    fn bsol_assignment_keys_use_the_property_legend_entry() {
        let text = "enabled = true";
        let tokens = build_bsol_semantic_tokens(
            text,
            &[BsolSemanticTokenCandidate { start: 0, end: 7, kind: BsolSemanticTokenKind::Property }],
            offset_to_position,
        );

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, 7);
        assert_eq!(tokens[0].token_modifiers_bitset, 1);
    }

    #[test]
    fn syntax_symbols_preserve_legend_order_and_delta_encoding() {
        let text = "fn first() {}\nstruct Second {}";
        let tokens = build_semantic_tokens(
            text,
            &[
                SyntaxSymbol { name: "first".into(), kind: AnalysisSymbolKind::Function, start: 3, end: 8 },
                SyntaxSymbol { name: "Second".into(), kind: AnalysisSymbolKind::Type, start: 21, end: 27 },
            ],
            &[],
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

    #[test]
    fn constant_symbols_use_the_variable_legend_entry() {
        let text = "const answer = 42";
        let tokens = build_semantic_tokens(
            text,
            &[SyntaxSymbol { name: "answer".into(), kind: AnalysisSymbolKind::Constant, start: 6, end: 12 }],
            &[],
            offset_to_position,
        );

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, 6); // VARIABLE follows the existing fixed legend entries.
        assert_eq!(tokens[0].token_modifiers_bitset, 1);
    }

    #[test]
    fn semantic_legend_exposes_the_decorator_token_for_attributes() {
        let legend = semantic_token_legend();
        assert_eq!(
            legend.token_types.get(super::TOKEN_TYPE_DECORATOR as usize),
            Some(&tower_lsp_server::ls_types::SemanticTokenType::DECORATOR)
        );
    }

    /// One attribute token type at every declaration site, including the old item sites.
    #[test]
    fn attribute_names_at_every_site_use_the_decorator_token() {
        let source = "[Meta]\npub attribute Hot(LambdaExpression, LambdaParameter, TypeParameter, LetStatement) { }\n\
[Shape]\npub type Box<[Hot] T> { T value, }\n\
[Entry]\ni64 Run() {\n    [Hot]\n    let square = [Hot] ([Hot] i64 x) => x * x;\n    return square(2);\n}\n";
        let program = beskid_analysis::services::parse_program(source).expect("attribute corpus parses");
        let names = crate::session::attribute_facts::attribute_names_for_program(&program);
        let tokens = build_semantic_tokens(source, &[], &names, offset_to_position);

        // Decode absolute (line, character) of every token.
        let mut line = 0u32;
        let mut character = 0u32;
        let mut decoded = Vec::new();
        for token in &tokens {
            if token.delta_line > 0 {
                line += token.delta_line;
                character = token.delta_start;
            } else {
                character += token.delta_start;
            }
            decoded.push((line, character, token.length, token.token_type, token.token_modifiers_bitset));
        }
        let at = |needle: &str, nth: usize| {
            let offset = source.match_indices(needle).nth(nth).expect("needle").0 + 1;
            let position = offset_to_position(source, offset);
            (position.line, position.character)
        };
        let decorator = |position: (u32, u32)| {
            decoded.iter().any(|(line, character, _, token_type, _)| {
                (*line, *character) == position && *token_type == super::TOKEN_TYPE_DECORATOR
            })
        };
        assert!(decorator(at("[Meta]", 0)), "attribute on an attribute declaration: {decoded:?}");
        assert!(decorator(at("[Shape]", 0)), "attribute on an item: {decoded:?}");
        assert!(decorator(at("[Entry]", 0)), "attribute on a function: {decoded:?}");
        assert!(decorator(at("[Hot]", 0)), "attribute on a type parameter: {decoded:?}");
        assert!(decorator(at("[Hot]", 1)), "attribute on a let statement: {decoded:?}");
        assert!(decorator(at("[Hot]", 2)), "attribute on a lambda: {decoded:?}");
        assert!(decorator(at("[Hot]", 3)), "attribute on a lambda parameter: {decoded:?}");
        let declared = source.find("Hot(").expect("declared name");
        let declared = offset_to_position(source, declared);
        assert!(
            decoded.iter().any(|(line, character, _, token_type, modifiers)| (*line, *character)
                == (declared.line, declared.character)
                && *token_type == super::TOKEN_TYPE_DECORATOR
                && *modifiers == 1),
            "declared attribute name carries the declaration modifier: {decoded:?}"
        );
        assert_eq!(decoded.iter().filter(|token| token.3 == super::TOKEN_TYPE_DECORATOR).count(), 8);
    }
}
