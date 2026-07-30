//! Shared sync/statement boundary primitives used by parse-recovery strategies.

use super::{
    expected_tokens,
    scan::{self, skip_ws},
};

/// Control-flow starters used by statement-level heuristics.
pub(crate) const CONTROL_FLOW_KEYWORDS: &[&str] = &["if", "else", "while", "for", "with", "match"];
pub(crate) const CONTROL_EXPRESSION_KEYWORDS: &[&str] = &["if", "while", "for", "match"];
pub(crate) const PRIMITIVE_TYPE_KEYWORDS: &[&str] =
    &["bool", "i32", "i64", "u8", "pointer", "word", "f64", "char", "string", "unit", "never"];
pub(crate) const TERMINATOR_KEYWORDS: &[&str] = &["let", "const", "return", "break", "continue", "launch"];

pub(crate) const KEYWORDS: &[&str] = &[
    "attribute",
    "const",
    "contract",
    "enum",
    "meta",
    "event",
    "host",
    "impl",
    "extend",
    "macro",
    "range",
    "global",
    "type",
    "in",
    "use",
    "mod",
    "test",
    "match",
    "skip",
    "startup",
    "init",
    "dispose",
    "scope",
    "registry",
    "when",
    "if",
    "else",
    "while",
    "for",
    "return",
    "break",
    "continue",
    "let",
    "mut",
    "with",
    "launch",
    "spawn",
    "async",
    "await",
    "code",
    "inject",
    "single",
    "transient",
    "parent",
    "pub",
    "as",
];

pub(crate) const RULE_KEYWORDS: &[(&str, &str)] = &[
    ("TypeDeclarationKeyword", "type"),
    ("EnumDeclarationKeyword", "enum"),
    ("ContractDeclarationKeyword", "contract"),
    ("AttributeDeclarationKeyword", "attribute"),
    ("ImplKeyword", "impl"),
    ("ExtendKeyword", "extend"),
    ("MatchKeyword", "match"),
    ("EventKeyword", "event"),
    ("WhenKeyword", "when"),
    ("IfKeyword", "if"),
    ("ElseKeyword", "else"),
    ("WhileKeyword", "while"),
    ("ForKeyword", "for"),
    ("InKeyword", "in"),
    ("ReturnKeyword", "return"),
    ("BreakKeyword", "break"),
    ("ContinueKeyword", "continue"),
    ("LetKeyword", "let"),
    ("ConstKeyword", "const"),
    ("MutKeyword", "mut"),
    ("ModKeyword", "mod"),
    ("UseKeyword", "use"),
    ("PubKeyword", "pub"),
    ("TestKeyword", "test"),
    ("SkipKeyword", "skip"),
    ("SpawnKeyword", "spawn"),
    ("AsyncKeyword", "async"),
    ("AwaitKeyword", "await"),
    ("CodeKeyword", "code"),
    ("HostKeyword", "host"),
    ("RegistryKeyword", "registry"),
    ("ScopeKeyword", "scope"),
    ("StartupKeyword", "startup"),
    ("InitKeyword", "init"),
    ("DisposeKeyword", "dispose"),
    ("WithKeyword", "with"),
    ("LaunchKeyword", "launch"),
    ("InjectKeyword", "inject"),
    ("SingleKeyword", "single"),
    ("TransientKeyword", "transient"),
    ("GlobalKeyword", "global"),
    ("ParentKeyword", "parent"),
];

pub(crate) fn keyword_rule_token(rule_name: &str) -> Option<&'static str> {
    RULE_KEYWORDS.iter().find_map(|(rule, token)| if *rule == rule_name { Some(*token) } else { None })
}

pub(crate) fn strip_keyword_suffix(rule_name: &str) -> &str {
    rule_name
        .strip_suffix("Keyword")
        .or_else(|| rule_name.strip_suffix("keyword"))
        .or_else(|| rule_name.strip_suffix("_Keyword"))
        .or_else(|| rule_name.strip_suffix("_keyword"))
        .unwrap_or(rule_name)
}

/// Derive the keyword surface form from a grammar rule name when no explicit mapping exists.
///
/// This primarily catches any future keyword-style rules without requiring a new manual map entry.
pub(crate) fn derive_keyword_rule_token(rule_name: &str) -> Option<String> {
    let base = rule_name
        .strip_suffix("Keyword")
        .or_else(|| rule_name.strip_suffix("keyword"))
        .or_else(|| rule_name.strip_suffix("_Keyword"))
        .or_else(|| rule_name.strip_suffix("_keyword"))?;
    if base.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(base.len());
    for (index, c) in base.chars().enumerate() {
        if c == '_' {
            break;
        }
        if index > 0 && c.is_ascii_uppercase() && out.chars().last().is_some_and(|prev| prev.is_ascii_lowercase()) {
            break;
        }
        if c == '_' {
            break;
        }
        out.push(c.to_ascii_lowercase());
    }

    if out.is_empty() || !out.chars().next().unwrap_or(' ').is_ascii_alphabetic() { None } else { Some(out) }
}

pub(crate) fn keyword_rule_token_or_derived(rule_name: &str) -> Option<&'static str> {
    let base = strip_keyword_suffix(rule_name);
    keyword_rule_token(base).or_else(|| {
        derive_keyword_rule_token(rule_name).map(|derived| {
            // New derived keyword entries are expected to be infrequent and only used as
            // fallback coverage for uncommon grammar additions.
            Box::leak(derived.into_boxed_str())
        } as &'static str)
    })
}

/// Full syntax-start keyword surface, used for sync boundaries.
pub(crate) const SYNC_KEYWORDS: &[&str] = KEYWORDS;

/// Keywords where a sync boundary should avoid semicolon insertion.
const NON_SEMI_SYNC_KEYWORDS: &[&str] = &["else", "in", "match", "when", "as"];

/// Top-level item starter surface shared by item-focused heuristics.
pub(crate) const ITEM_START_KEYWORDS: &[&str] = &[
    "host",
    "macro",
    "impl",
    "extend",
    "type",
    "enum",
    "contract",
    "test",
    "attribute",
    "mod",
    "use",
    "scope",
    "registry",
    "meta",
    "skip",
    "single",
    "transient",
];

/// Keywords that introduce a brace-delimited item body and should prefer body-close recovery.
pub(crate) const ITEM_BODY_OPEN_KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "impl",
    "extend",
    "attribute",
    "macro",
    "test",
    "host",
    "mod",
    "scope",
    "registry",
    "meta",
    "skip",
    "single",
    "transient",
];

pub(crate) const RULE_FAMILY_FALLBACKS: &[(&str, &str, &str, u8)] = &[
    ("Expression", "true", "inserted parser-expected expression", 45),
    ("Statement", "true;", "inserted parser-expected statement", 44),
    ("Body", "{ }", "inserted parser-expected block body", 44),
    ("Definition", "word() { }", "inserted parser-expected definition", 44),
    ("Declaration", "let value = 0;", "inserted parser-expected declaration", 44),
    ("Clause", "=>", "inserted parser-expected clause", 42),
    ("Operator", "=", "inserted parser-expected operator", 42),
    ("Separator", ";", "inserted parser-expected separator", 40),
    ("List", "value", "inserted parser-expected list entry", 42),
    ("Field", "field: word", "inserted parser-expected structure field", 46),
    ("Path", "path", "inserted parser-expected path", 46),
    ("Type", "word", "inserted parser-expected type", 46),
    ("Parameter", "word", "inserted parser-expected parameter", 44),
    ("Argument", "word", "inserted parser-expected parameter", 44),
    ("Item", "let value = 0", "inserted parser-expected item", 44),
    ("Literal", "0", "inserted parser-expected literal", 44),
    ("Number", "0", "inserted parser-expected literal", 44),
    ("Comment", "///", "inserted parser-expected comment", 40),
    ("Identifier", "value", "inserted parser-expected identifier", 40),
    ("LanguageTag", "txt", "inserted parser-expected code language tag", 40),
    ("StringText", "text", "inserted parser-expected string text", 40),
    ("StringEscape", "\\", "inserted parser-expected string escape", 38),
    ("Pattern", "_", "inserted parser-expected pattern", 45),
    ("Variant", "Value", "inserted parser-expected variant", 40),
    ("Constructor", "path()", "inserted parser-expected constructor", 40),
    ("Qualifier", "global", "inserted parser-expected qualifier", 40),
];

pub(crate) const BODY_OPENING_TOKEN_CLASSES: &[expected_tokens::ReplacementTokenClass] = &[
    expected_tokens::ReplacementTokenClass::Identifier,
    expected_tokens::ReplacementTokenClass::Number,
    expected_tokens::ReplacementTokenClass::StringLike,
    expected_tokens::ReplacementTokenClass::Keyword,
    expected_tokens::ReplacementTokenClass::Delimiter,
];

pub(crate) fn rule_family_fallback(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    if let Some(fallback) = RULE_FAMILY_FALLBACKS.iter().find_map(|(name, token, reason, confidence)| {
        rule_name.contains(name).then_some((*token, *reason, *confidence))
    }) {
        return Some(fallback);
    }

    infer_rule_family_from_name(rule_name).or_else(|| infer_rule_name_shape(rule_name))
}

fn infer_rule_family_from_name(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    if rule_name.ends_with("Statement") || rule_name.ends_with("statement") {
        return Some(("true;", "inserted parser-expected statement (shape)", 44));
    }
    if rule_name.ends_with("Expression") || rule_name.ends_with("expression") {
        return Some(("true", "inserted parser-expected expression (shape)", 44));
    }
    if rule_name.ends_with("Definition")
        || rule_name.ends_with("definition")
        || rule_name.ends_with("Decl")
        || rule_name.ends_with("decl")
        || rule_name.ends_with("Declaration")
        || rule_name.ends_with("declaration")
    {
        return Some(("let value = 0", "inserted parser-expected definition (shape)", 44));
    }
    if rule_name.ends_with("List") || rule_name.ends_with("list") {
        return Some(("value", "inserted parser-expected list entry (shape)", 42));
    }
    if rule_name.ends_with("ItemWithDocs")
        || rule_name.ends_with("itemwithdocs")
        || rule_name == "InnerItem"
        || rule_name == "inneritem"
        || rule_name == "ItemList"
        || rule_name == "itemlist"
        || rule_name.ends_with("Item")
        || rule_name.ends_with("item")
    {
        return Some(("let value = 0", "inserted parser-expected item (shape)", 42));
    }
    if rule_name.ends_with("Constructor")
        || rule_name.ends_with("constructor")
        || rule_name.ends_with("enum_constructor_nullary")
        || rule_name.ends_with("enum_constructor_with_args")
    {
        return Some(("path()", "inserted parser-expected constructor (shape)", 40));
    }
    if rule_name == "CodeExpression" || rule_name == "codeexpression" {
        return Some(("path()", "inserted parser-expected code expression (shape)", 40));
    }
    None
}

fn infer_rule_name_shape(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    let name = rule_name.to_ascii_lowercase();
    if name.contains("list") {
        return Some(("value", "inserted parser-expected list entry (shape fragment)", 40));
    }
    if name.contains("pattern") {
        return Some(("_", "inserted parser-expected pattern (shape)", 42));
    }
    if name.contains("qual") {
        return Some(("global", "inserted parser-expected qualifier (shape)", 40));
    }
    if name.contains("body") {
        return Some(("{ }", "inserted parser-expected block body (shape)", 40));
    }
    if name.contains("keyword") {
        return keyword_rule_token_or_derived(strip_keyword_suffix(rule_name))
            .map(|keyword| (keyword, "inserted parser-expected rule shape keyword", 36));
    }
    None
}

pub(crate) fn is_line_start(source: &str, pos: usize) -> bool {
    let mut cursor = pos.min(source.len());
    while cursor > 0 && source.as_bytes()[cursor - 1].is_ascii_whitespace() {
        if source.as_bytes()[cursor - 1] == b'\n' {
            return true;
        }
        cursor -= 1;
    }
    cursor == 0 || source.as_bytes()[cursor - 1] == b'\n'
}

pub(crate) fn is_for_clause_in_keyword(source: &str, pos: usize) -> bool {
    let mut line_start = pos;
    while line_start > 0
        && source.as_bytes()[line_start - 1] != b'\n'
        && source.as_bytes()[line_start - 1].is_ascii_whitespace()
    {
        line_start -= 1;
    }
    if line_start > 0
        && !source.as_bytes()[line_start - 1].is_ascii_whitespace()
        && source.as_bytes()[line_start - 1] != b'\n'
    {
        return false;
    }

    let prev = line_start.saturating_sub(1);
    if prev == 0 {
        return false;
    }

    let mut token_start = prev;
    while token_start > 0 && scan::is_ident_continue(source.as_bytes()[token_start - 1]) {
        token_start -= 1;
    }
    &source[token_start..prev] == "for"
}

pub(crate) fn is_recoverable_sync_keyword(source: &str, pos: usize, keyword: &str) -> bool {
    if keyword == "as" {
        return is_line_start(source, pos);
    }
    if keyword == "in" {
        return !is_for_clause_in_keyword(source, pos);
    }
    true
}

pub(crate) fn is_recoverable_statement_start(source: &str, pos: usize, keyword: &str) -> bool {
    match keyword {
        "as" | "mut" | "pub" => is_line_start(source, pos),
        "in" => !is_for_clause_in_keyword(source, pos),
        _ => true,
    }
}

pub(crate) fn should_skip_sync_semicolon(keyword: &str) -> bool {
    NON_SEMI_SYNC_KEYWORDS.iter().any(|item| item == &keyword)
}

pub(crate) fn is_top_level_at(source: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let (paren, bracket, brace, angle) = scan::unbalanced_delimiters(source, pos);
    paren == 0 && bracket == 0 && brace == 0 && angle == 0
}

pub(crate) fn recovery_scan_pos(source: &str, error_pos: usize) -> usize {
    if error_pos == 0 {
        return source.trim_end().len();
    }
    error_pos.min(source.len())
}

pub(crate) fn recovery_insert_position(source: &str, boundary_pos: usize) -> usize {
    let mut pos = boundary_pos.min(source.len());
    while pos > 0 && source.as_bytes()[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    pos
}

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

pub(crate) fn is_recoverable_identifier_statement_starter(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    if !is_top_level_at(source, pos) {
        return false;
    }
    if !is_line_start(source, pos) {
        return false;
    }
    let bytes = source.as_bytes();
    if !scan::is_ident_start(bytes[pos]) && bytes[pos] != b'_' {
        return false;
    }
    for keyword in SYNC_KEYWORDS {
        if scan::keyword_at(source, pos, keyword) {
            return false;
        }
    }
    true
}

pub(crate) fn is_recoverable_expression_statement_starter(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    if !is_line_start(source, pos) {
        return false;
    }
    if !scan::looks_like_expression_start(source, pos) {
        return false;
    }
    let bytes = source.as_bytes();
    if bytes[pos] == b'_' || scan::is_ident_start(bytes[pos]) {
        let end = scan::skip_identifier(source, pos);
        if end > pos && is_keyword_text(&source[pos..end]) {
            return false;
        }
    }
    true
}

pub(crate) fn recovery_source_has_fallback_control_flow_hint(
    source: &str,
    error_pos: usize,
    keywords: &[&str],
) -> bool {
    let error_pos = error_pos.min(source.len());
    let trimmed = source[..error_pos].trim_end();
    if trimmed.trim().is_empty() {
        return false;
    }
    let mut latest = None::<(usize, &str)>;
    let bytes = trimmed.as_bytes();
    for &keyword in keywords {
        let Some(kw_pos) = scan::find_keyword_backward(trimmed, trimmed.len(), keyword) else {
            continue;
        };
        if kw_pos > 0 && scan::is_ident_continue(bytes[kw_pos - 1]) {
            continue;
        }
        let kw_end = kw_pos + keyword.len();
        if kw_end < bytes.len() && scan::is_ident_continue(bytes[kw_end]) {
            continue;
        }
        match latest {
            Some((current, _)) if current >= kw_pos => {}
            _ => latest = Some((kw_pos, keyword)),
        }
    }
    let Some((kw_pos, keyword)) = latest else {
        return false;
    };

    let after_kw = skip_ws(trimmed, kw_pos + keyword.len());
    if after_kw >= trimmed.len() {
        return false;
    }
    if trimmed.as_bytes()[kw_pos + keyword.len()] == b'=' {
        return false;
    }
    let tail = trimmed[after_kw..trimmed.len()].trim();
    if tail.is_empty() || tail.starts_with("=>") || tail.ends_with(':') || tail.ends_with('=') || tail.ends_with("=>") {
        return false;
    }
    if tail.contains('{') {
        return false;
    }
    true
}

/// Shared sync-boundary predicate used by sync and statement-start discovery.
///
/// Returns `Some` when `pos` looks like a recoverable statement/expression boundary.
///
/// The first tuple item is:
/// - `Some(keyword)` if the boundary starts with a sync keyword from `sync_keywords`.
/// - `None` for non-keyword statement/expression starters.
pub(crate) fn recoverable_sync_boundary_start<'a>(
    source: &str,
    pos: usize,
    sync_keywords: &'a [&'a str],
) -> Option<(Option<&'a str>, bool)> {
    for &keyword in sync_keywords {
        if scan::keyword_at(source, pos, keyword) && is_recoverable_sync_keyword(source, pos, keyword) {
            return Some((Some(keyword), should_skip_sync_semicolon(keyword)));
        }
    }

    if is_recoverable_identifier_statement_starter(source, pos)
        || is_recoverable_expression_statement_starter(source, pos)
    {
        return Some((None, false));
    }

    None
}

/// Scan for recoverable top-level statement starts in syntax order.
pub(crate) fn top_level_statement_starts(source: &str, from: usize, keywords: &[&str]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut pos = from.min(source.len());
    while pos < source.len() {
        pos = skip_ws(source, pos);
        if pos >= source.len() {
            break;
        }

        let Some((keyword, _)) = recoverable_sync_boundary_start(source, pos, keywords) else {
            pos += 1;
            continue;
        };

        if let Some(keyword) = keyword {
            if is_recoverable_statement_start(source, pos, keyword) {
                starts.push(pos);
                pos = pos.saturating_add(keyword.len());
                continue;
            }
        } else {
            starts.push(pos);
            let next_pos = scan::next_token_start(source, pos + 1).unwrap_or(source.len());
            pos = next_pos.max(pos + 1);
            continue;
        }

        pos += 1;
    }
    starts
}

/// Find the most recent unmatched delimiter opener for a delimiter pair before `through`.
pub(crate) fn find_unclosed_delimiter_before(source: &str, through: usize, open: u8, close: u8) -> Option<usize> {
    let through = through.min(source.len());
    if through == 0 || open == close {
        return None;
    }

    let mut stack: Vec<usize> = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;

    while pos < through {
        match bytes[pos] {
            b'"' | b'\'' => {
                pos = scan::skip_string_or_char(source, pos);
                continue;
            }
            b'/' if pos + 1 < through && bytes[pos + 1] == b'/' => {
                pos += 2;
                while pos < through && bytes[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            b'/' if pos + 1 < through && bytes[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < through && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                    pos += 1;
                }
                pos = (pos + 2).min(through);
                continue;
            }
            _ => {}
        }

        if bytes[pos] == open {
            stack.push(pos);
            pos += 1;
            continue;
        }

        if bytes[pos] == close {
            let _ = stack.pop();
            pos += 1;
            continue;
        }

        pos += 1;
    }

    stack.pop()
}

/// Return the byte position of a trailing `separator` just before the next
/// top-level close delimiter for the tracked list.
pub(crate) fn trailing_separator_before_list_close(
    source: &str,
    open_pos: usize,
    through: usize,
    open: u8,
    close: u8,
    separator: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if open_pos >= source.len() || open == close {
        return None;
    }

    let mut i = open_pos + 1;
    let limit = through.min(source.len());
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut last_top_level: Option<(usize, u8)> = None;

    while i < limit {
        match bytes[i] {
            b'"' | b'\'' => {
                i = scan::skip_string_or_char(source, i);
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                i += 2;
                while i < limit && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(limit);
                continue;
            }
            b'(' => {
                paren += 1;
            }
            b')' => {
                if paren > 0 {
                    paren -= 1;
                } else if close == b')' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if paren == 0 && close != b')' && bracket == 0 && brace == 0 && angle == 0 {
                    return None;
                }
            }
            b'[' => {
                bracket += 1;
            }
            b']' => {
                if bracket > 0 {
                    bracket -= 1;
                } else if close == b']' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if bracket == 0 && close != b']' && paren == 0 && brace == 0 && angle == 0 {
                    return None;
                }
            }
            b'{' => {
                brace += 1;
            }
            b'}' => {
                if brace > 0 {
                    brace -= 1;
                } else if close == b'}' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if brace == 0 && close != b'}' && paren == 0 && bracket == 0 && angle == 0 {
                    return None;
                }
            }
            b'<' => {
                angle += 1;
            }
            b'>' => {
                if angle > 0 {
                    angle -= 1;
                } else if close == b'>' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if angle == 0 && close != b'>' && paren == 0 && bracket == 0 && brace == 0 {
                    return None;
                }
            }
            _ => {}
        }

        if paren == 0 && bracket == 0 && brace == 0 && angle == 0 && !bytes[i].is_ascii_whitespace() {
            last_top_level = Some((i, bytes[i]));
        }

        i += 1;
    }

    last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos)
}

/// Find a matching close delimiter for `open_pos` by scanning forward with balanced nesting.
pub(crate) fn matching_delimiter_close(source: &str, open_pos: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if open_pos >= source.len() || open == close || bytes.get(open_pos) != Some(&open) {
        return None;
    }

    let mut i = open_pos + 1;
    let limit = source.len();
    let mut depth = 0i32;

    while i < limit {
        match bytes[i] {
            b'"' | b'\'' => {
                i = scan::skip_string_or_char(source, i);
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                i += 2;
                while i < limit && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(limit);
                continue;
            }
            _ => {}
        }

        if bytes[i] == open {
            depth += 1;
            i += 1;
            continue;
        }

        if bytes[i] == close {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }

        i += 1;
    }

    None
}

pub(crate) fn control_flow_keyword_len(source: &str, kw_pos: usize) -> Option<usize> {
    for &keyword in CONTROL_FLOW_KEYWORDS {
        if source[kw_pos..].starts_with(keyword) {
            return Some(keyword.len());
        }
    }
    None
}

pub(crate) fn is_keyword_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    KEYWORDS.contains(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

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
