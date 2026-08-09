//! Shared sync/statement boundary primitives used by parse-recovery strategies.

mod boundaries;
mod keyword_rules;
mod list_separators;
mod scanner;
mod token_expectations;

pub(crate) use boundaries::{
    control_flow_keyword_len, is_for_clause_in_keyword, is_keyword_text, is_line_start,
    is_recoverable_expression_statement_starter, is_recoverable_identifier_statement_starter,
    is_recoverable_statement_start, is_recoverable_sync_keyword, is_top_level_at, recoverable_sync_boundary_start,
    recovery_insert_position, recovery_scan_pos, recovery_source_has_fallback_control_flow_hint,
    should_skip_sync_semicolon, top_level_statement_starts,
};
pub(crate) use keyword_rules::{
    CONTROL_EXPRESSION_KEYWORDS, CONTROL_FLOW_KEYWORDS, ITEM_BODY_OPEN_KEYWORDS, ITEM_START_KEYWORDS, KEYWORDS,
    PRIMITIVE_TYPE_KEYWORDS, RULE_FAMILY_FALLBACKS, RULE_KEYWORDS, SYNC_KEYWORDS, TERMINATOR_KEYWORDS,
    derive_keyword_rule_token, keyword_rule_token, keyword_rule_token_or_derived, rule_family_fallback,
    strip_keyword_suffix,
};
pub(crate) use list_separators::trailing_separator_before_list_close;
pub(crate) use scanner::{find_unclosed_delimiter_before, matching_delimiter_close};
pub(crate) use token_expectations::{
    BODY_OPENING_TOKEN_CLASSES, recovery_expected_or_follow_token_has_any_class,
    recovery_expected_or_follow_token_has_body_hint, recovery_expected_token_has_any_class,
    recovery_expected_token_is_expected, recovery_follow_token_has_any_class, recovery_follow_token_is_expected,
    recovery_follow_tokens, recovery_sync_keywords,
};

#[cfg(test)]
mod tests {
    use super::{KEYWORDS, PRIMITIVE_TYPE_KEYWORDS};

    #[test]
    fn all_grammar_keyword_rules_are_recovery_keywords() {
        const GRAMMAR: &str = include_str!("../../beskid.pest");

        let mut parsed = Vec::<String>::new();
        for line in GRAMMAR.lines() {
            let line = line.trim();
            if !line.contains("Keyword") {
                continue;
            }

            let Some((rule, rest)) = line.split_once(" = ") else {
                continue;
            };

            if rule == "Keyword" || !rule.ends_with("Keyword") {
                continue;
            }

            let Some(first_quote) = rest.find('"') else {
                continue;
            };

            let after_first = &rest[first_quote + 1..];
            let Some(second_quote) = after_first.find('"') else {
                continue;
            };

            parsed.push(after_first[..second_quote].to_string());
        }

        for keyword in parsed {
            if !KEYWORDS.contains(&keyword.as_str()) {
                panic!("recovery keyword list missing grammar keyword surface `{keyword}`");
            }
        }
    }

    #[test]
    fn primitive_type_keywords_cover_grammar_surface() {
        const GRAMMAR: &str = include_str!("../../beskid.pest");

        let Some((_, raw)) =
            GRAMMAR.lines().find_map(|line| line.split_once(" = ").filter(|(lhs, _)| *lhs == "PrimitiveType"))
        else {
            panic!("primitive type rule not found in grammar");
        };

        let rhs = raw.trim().trim_start_matches('{').trim_end_matches('}');
        let mut grammar_types: Vec<&str> = rhs
            .split('|')
            .map(str::trim)
            .filter_map(|token| token.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')))
            .filter(|token| !token.is_empty())
            .collect();

        let mut expected: Vec<&str> = PRIMITIVE_TYPE_KEYWORDS.to_vec();
        grammar_types.sort_unstable();
        expected.sort_unstable();

        assert_eq!(grammar_types, expected);
    }
}
