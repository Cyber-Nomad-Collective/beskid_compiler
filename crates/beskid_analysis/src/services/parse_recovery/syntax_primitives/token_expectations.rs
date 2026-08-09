use super::super::expected_tokens;
use super::keyword_rules::SYNC_KEYWORDS;

pub(crate) const BODY_OPENING_TOKEN_CLASSES: &[expected_tokens::ReplacementTokenClass] = &[
    expected_tokens::ReplacementTokenClass::Identifier,
    expected_tokens::ReplacementTokenClass::Number,
    expected_tokens::ReplacementTokenClass::StringLike,
    expected_tokens::ReplacementTokenClass::Keyword,
    expected_tokens::ReplacementTokenClass::Delimiter,
];

/// Return a deduplicated keyword-augmented sync keyword set for recovery.
///
/// This keeps statement/item boundary discovery aligned across sync and statement
/// recovery phases by sharing the same recovery keyword surface.
pub(crate) fn recovery_sync_keywords(parse_error: &pest::error::Error<crate::parser::Rule>) -> Vec<&'static str> {
    let mut keywords = SYNC_KEYWORDS.to_vec();
    for keyword in expected_tokens::expected_keyword_tokens(parse_error) {
        if !keywords.contains(&keyword) {
            keywords.push(keyword);
        }
    }
    keywords
}

/// Derive a compact follow-token recovery set from parser error expectations.
///
/// This list is shared by parser-sync and syntax heuristics so different recovery
/// phases make decisions from the same follow-set signal.
pub(crate) fn recovery_follow_tokens(parse_error: &pest::error::Error<crate::parser::Rule>) -> Vec<&'static str> {
    let mut tokens: Vec<&'static str> = expected_tokens::expected_token_candidates(parse_error)
        .into_iter()
        .filter_map(|(token, _, _)| {
            if token.is_empty() || token.len() > 6 {
                return None;
            }
            if token == "{" || token == "(" || token == "[" || token == "<" {
                return None;
            }

            let token_class = expected_tokens::replacement_token_class(token);
            if token_class == expected_tokens::ReplacementTokenClass::Delimiter
                || token_class == expected_tokens::ReplacementTokenClass::Operator
                || token_class == expected_tokens::ReplacementTokenClass::Keyword
            {
                Some(token)
            } else {
                None
            }
        })
        .collect();

    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

pub(crate) fn recovery_follow_token_is_expected(
    parse_error: &pest::error::Error<crate::parser::Rule>,
    needle: &str,
) -> bool {
    recovery_follow_tokens(parse_error).contains(&needle)
}

pub(crate) fn recovery_expected_token_is_expected(
    parse_error: &pest::error::Error<crate::parser::Rule>,
    needle: &str,
) -> bool {
    expected_tokens::expected_token_candidates(parse_error).iter().any(|(token, _, _)| *token == needle)
}

pub(crate) fn recovery_expected_token_has_any_class(
    parse_error: &pest::error::Error<crate::parser::Rule>,
    needle_classes: &[expected_tokens::ReplacementTokenClass],
) -> bool {
    expected_tokens::expected_token_candidates(parse_error).iter().any(|(token, _, _)| {
        needle_classes.iter().any(|needle_class| expected_tokens::replacement_token_class(token) == *needle_class)
    })
}

pub(crate) fn recovery_follow_token_has_any_class(
    parse_error: &pest::error::Error<crate::parser::Rule>,
    needle_classes: &[expected_tokens::ReplacementTokenClass],
) -> bool {
    recovery_follow_tokens(parse_error).iter().any(|token| {
        needle_classes.iter().any(|needle_class| expected_tokens::replacement_token_class(token) == *needle_class)
    })
}

pub(crate) fn recovery_expected_or_follow_token_has_any_class(
    parse_error: &pest::error::Error<crate::parser::Rule>,
    needle_classes: &[expected_tokens::ReplacementTokenClass],
) -> bool {
    recovery_expected_token_has_any_class(parse_error, needle_classes)
        || recovery_follow_token_has_any_class(parse_error, needle_classes)
}

pub(crate) fn recovery_expected_or_follow_token_has_body_hint(
    parse_error: &pest::error::Error<crate::parser::Rule>,
) -> bool {
    recovery_expected_or_follow_token_has_any_class(parse_error, BODY_OPENING_TOKEN_CLASSES)
        || recovery_expected_token_is_expected(parse_error, "{")
        || recovery_follow_token_is_expected(parse_error, "{")
}
