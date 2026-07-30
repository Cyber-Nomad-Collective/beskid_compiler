//! Shared recovery heuristics used by sync/list recovery strategies.

use crate::parser::Rule;

use super::{expected_tokens, scan};

/// ANTLR-style single-token deletion priority baseline from recover-inline flow.
pub(crate) const PRI_SYNC_SINGLE_TOKEN_DELETE: u8 = 16;

pub(crate) fn is_single_delete_candidate_token(token: &str) -> bool {
    let Some(byte) = token.as_bytes().first().copied() else {
        return false;
    };
    scan::is_delimiter_byte(byte) || scan::is_operator_byte(byte)
}

pub(crate) fn can_recover_single_token_deletion(
    source: &str,
    next_start: usize,
    unexpected: &str,
    next_token: &str,
    parse_error: &pest::error::Error<Rule>,
) -> bool {
    // Source of truth for ANTLR/Bison-style single-token deletion checks:
    // the parser can often continue if it can validate LA(1) with the expected
    // symbol set after discarding the unexpected token.
    let next_class = expected_tokens::replacement_token_class(next_token);
    let unexpected_class = expected_tokens::replacement_token_class(unexpected);
    let (paren, bracket, brace, angle) = scan::unbalanced_delimiters(source, next_start);
    let inside_nested_structure = paren > 0 || bracket > 0 || brace > 0 || angle > 0;
    let next_is_expr_start = scan::looks_like_expression_start(source, next_start);

    let expected_candidates = expected_tokens::expected_token_candidates(parse_error);
    if expected_candidates.is_empty() {
        if matches!(
            unexpected_class,
            expected_tokens::ReplacementTokenClass::Delimiter | expected_tokens::ReplacementTokenClass::Operator
        ) && inside_nested_structure
            && (next_is_expr_start
                || matches!(
                    next_class,
                    expected_tokens::ReplacementTokenClass::Delimiter
                        | expected_tokens::ReplacementTokenClass::Operator
                ))
        {
            return true;
        }
        return false;
    }

    let is_expected_next = expected_candidates.iter().any(|(expected, _, _)| {
        *expected == next_token || expected.starts_with(next_token) || {
            let expected_class = expected_tokens::replacement_token_class(expected);
            expected_tokens::replacement_tokens_compatible(expected_class, next_class)
        }
    });
    if is_expected_next {
        return true;
    }

    let expected_allows_expression_start = expected_candidates.iter().any(|(expected, _, _)| {
        let expected_class = expected_tokens::replacement_token_class(expected);
        matches!(
            expected_class,
            expected_tokens::ReplacementTokenClass::Identifier
                | expected_tokens::ReplacementTokenClass::Number
                | expected_tokens::ReplacementTokenClass::StringLike
        )
    });
    let expected_allows_delimiter = expected_candidates.iter().any(|(expected, _, _)| {
        expected_tokens::replacement_token_class(expected) == expected_tokens::ReplacementTokenClass::Delimiter
    });

    if matches!(
        unexpected_class,
        expected_tokens::ReplacementTokenClass::Delimiter | expected_tokens::ReplacementTokenClass::Operator
    ) && next_is_expr_start
        && inside_nested_structure
    {
        return true;
    }

    if matches!(
        unexpected_class,
        expected_tokens::ReplacementTokenClass::Delimiter | expected_tokens::ReplacementTokenClass::Operator
    ) && inside_nested_structure
        && (expected_allows_expression_start
            || expected_allows_delimiter
            || matches!(
                next_class,
                expected_tokens::ReplacementTokenClass::Delimiter | expected_tokens::ReplacementTokenClass::Operator
            ))
    {
        return true;
    }

    if expected_allows_delimiter
        && next_is_expr_start
        && matches!(next_class, expected_tokens::ReplacementTokenClass::Delimiter)
    {
        return true;
    }

    unexpected == next_token
        && expected_candidates.iter().any(|(expected, _, _)| {
            expected_tokens::replacement_token_class(expected) == expected_tokens::ReplacementTokenClass::Delimiter
                || expected_tokens::replacement_token_class(expected)
                    == expected_tokens::ReplacementTokenClass::Operator
                || matches!(
                    unexpected_class,
                    expected_tokens::ReplacementTokenClass::Delimiter
                        | expected_tokens::ReplacementTokenClass::Operator
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{BeskidParser, Rule};
    use pest::Parser;

    #[test]
    fn keeps_deletion_candidate_for_operator_or_delimiter_tokens() {
        assert!(is_single_delete_candidate_token(","));
        assert!(is_single_delete_candidate_token("]"));
        assert!(is_single_delete_candidate_token("=="));
        assert!(!is_single_delete_candidate_token("word"));
        assert!(!is_single_delete_candidate_token("0"));
    }

    #[test]
    fn allows_expected_next_token_after_redundant_delimiter() {
        let cases = vec!["let value = f(1,,2)", "let value = f(1,,2);"];
        for source in cases {
            let parse_error =
                BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
            let comma_pos = source.match_indices(",").nth(1).map(|(index, _)| index).expect("missing duplicate comma");
            let next_start = source[comma_pos + 1..]
                .find(|c: char| !c.is_whitespace())
                .map(|offset| comma_pos + 1 + offset)
                .expect("missing next token");
            let next_token_end = source[next_start..]
                .find(|c: char| c.is_ascii_whitespace() || c == ';' || c == ')' || c == '}' || c == ']' || c == '>')
                .map(|offset| next_start + offset)
                .unwrap_or(source.len())
                .max(next_start + 1);
            let next_token = &source[next_start..next_token_end];
            assert!(next_start < source.len(), "missing next token for case {source}");
            let expected = expected_tokens::expected_token_candidates(&parse_error);
            assert!(
                can_recover_single_token_deletion(source, next_start, ",", next_token, &parse_error),
                "source={source} variant={:?} expected={expected:?}",
                parse_error.variant
            );
        }
    }
}
