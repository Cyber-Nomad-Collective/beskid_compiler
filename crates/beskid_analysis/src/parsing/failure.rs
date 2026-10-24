//! What a failed parse expected, what it found, and where.
//!
//! The grammar error from pest names rules and points at the start of the furthest rule that
//! failed. [`ParseFailure`] re-runs the failed parse with pest's attempt tracking and reads the
//! furthest position the parser reached, the literal tokens it would have accepted there and the
//! innermost rules it was trying, then names them in words ([`crate::rule_words`]).

use std::sync::Mutex;

use pest::Parser;
use pest::error::InputLocation;

use crate::parser::{BeskidParser, Rule};
use crate::rule_words::{Words, rule_words};

use super::syntax_context::{RESERVED_WORDS, ScanContext, Tok, token_at, tokens_before};

/// Number of parses that currently need pest's attempt tracking. Tracking is a process-wide pest
/// switch; it stays on while any parse here needs it, so parallel parses never turn it off early.
static DETAIL_USERS: Mutex<usize> = Mutex::new(0);

/// Run `parse` with pest's attempt tracking on (slower; only for a parse already known to fail).
pub fn with_error_detail<T>(parse: impl FnOnce() -> T) -> T {
    {
        let mut users = DETAIL_USERS.lock().unwrap_or_else(|poison| poison.into_inner());
        *users += 1;
        pest::set_error_detail(true);
    }
    let result = parse();
    {
        let mut users = DETAIL_USERS.lock().unwrap_or_else(|poison| poison.into_inner());
        *users = users.saturating_sub(1);
        if *users == 0 {
            pest::set_error_detail(false);
        }
    }
    result
}

/// Parse `source` as `rule` and return the detailed error, or `None` when it parses.
pub fn detailed_error(rule: Rule, source: &str) -> Option<pest::error::Error<Rule>> {
    with_error_detail(|| BeskidParser::parse(rule, source).err())
}

/// One thing the parser would have accepted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expectation {
    /// A literal token, without backticks (`;`, `(`, `let`).
    Token(String),
    /// Words for a rule ("a name", "an expression").
    Thing(String),
    /// Words for a construct, used only when nothing more precise is known.
    Construct(String),
}

/// A parse failure in words.
#[derive(Debug, Clone)]
pub struct ParseFailure {
    /// Offset of the furthest point the parser reached.
    pub position: usize,
    /// The token found there; `None` at end of input.
    pub found: Option<Tok>,
    /// What could have followed, deduplicated.
    pub expected: Vec<Expectation>,
    /// The constructs open at the failure.
    pub context: ScanContext,
}

/// Tokens the grammar matches between other tokens (whitespace and comments).
fn is_trivia_token(token: &str) -> bool {
    matches!(token, " " | "\t" | "\n" | "\r" | "\r\n" | "//" | "///" | "////" | "/*" | "*/")
        || token == "BUILTIN_RULE"
        || (token.chars().count() == 4 && token.get(1..3) == Some(".."))
}

impl ParseFailure {
    /// Describe the failure of parsing `source` as `rule`; `error` is the grammar error already in hand.
    pub fn analyze(rule: Rule, source: &str, error: &pest::error::Error<Rule>) -> Self {
        let detailed =
            if error.parse_attempts().is_some() { Some(error.clone()) } else { detailed_error(rule, source) };
        let detailed = detailed.as_ref().unwrap_or(error);

        let (position, mut expected) = match detailed.parse_attempts() {
            Some(attempts) => {
                let mut expected = Vec::new();
                for token in attempts.expected_tokens() {
                    let token = token.to_string();
                    if !is_trivia_token(&token) {
                        expected.push(Expectation::Token(token));
                    }
                }
                for stack in &attempts.call_stacks {
                    if let Some(rule) = stack.deepest.get_rule() {
                        push_rule(&mut expected, *rule);
                    }
                }
                (attempts.max_position, expected)
            }
            None => {
                let position = match detailed.location {
                    InputLocation::Pos(pos) => pos,
                    InputLocation::Span((start, _)) => start,
                };
                let mut expected = Vec::new();
                if let pest::error::ErrorVariant::ParsingError { positives, .. } = &detailed.variant {
                    for rule in positives {
                        push_rule(&mut expected, *rule);
                    }
                }
                (position, expected)
            }
        };
        let position = position.min(source.len());
        expected.sort();
        expected.dedup();
        let mut found = token_at(source, position);
        // A reserved word where a name belongs fails just after the word (the grammar checks the
        // word boundary there); the word itself is what the user has to change.
        if let Some(word) = tokens_before(source, position).pop()
            && word.end == position
            && word.word().is_some_and(|text| RESERVED_WORDS.contains(&text))
            && expected.iter().any(|expectation| matches!(expectation, Expectation::Thing(t) if t == "a name"))
        {
            found = Some(word);
        }
        let context = ScanContext::at(source, found.as_ref().map_or(source.len(), |tok| tok.start));
        Self { position: found.as_ref().map_or(source.len(), |tok| tok.start), found, expected, context }
    }

    /// Whether `token` (without backticks) is among the expected tokens.
    pub fn expects_token(&self, token: &str) -> bool {
        self.expected.iter().any(|expectation| matches!(expectation, Expectation::Token(t) if t == token))
    }

    /// Whether the rule words `thing` are among the expectations.
    pub fn expects_thing(&self, thing: &str) -> bool {
        self.expected.iter().any(|expectation| matches!(expectation, Expectation::Thing(t) if t == thing))
    }

    /// The found token in words: "`{`", "a string", "end of file".
    pub fn found_words(&self, source: &str) -> String {
        self.found.as_ref().map_or_else(|| "end of file".to_string(), |tok| tok.describe(source))
    }

    /// The expectations in words: "`;`", "`(` or `<`", "a name, `mut` or `=`".
    pub fn expected_words(&self, source: &str) -> String {
        render_expected(&self.expected, self.found_on_new_line(source))
    }

    /// The found token starts a later line than the token before it.
    pub fn found_on_new_line(&self, source: &str) -> bool {
        let Some(end) = super::syntax_context::previous_token_end(source, self.position) else {
            return false;
        };
        source[end..self.position].contains('\n')
    }
}

fn push_rule(expected: &mut Vec<Expectation>, rule: Rule) {
    match rule_words(rule) {
        Words::Token(text) => expected.push(Expectation::Token(text.to_string())),
        Words::Thing(text) => expected.push(Expectation::Thing(text.to_string())),
        Words::Construct(text) => expected.push(Expectation::Construct(text.to_string())),
    }
}

const STATEMENT_KEYWORDS: &[&str] =
    &["let", "return", "if", "while", "for", "break", "continue", "use", "with", "launch", "mut"];
const ITEM_KEYWORDS: &[&str] =
    &["type", "enum", "contract", "impl", "extend", "mod", "use", "host", "test", "pub", "const", "attribute", "macro"];
const BINARY_OPERATORS: &[&str] = &[
    "+", "-", "*", "/", "%", "==", "!=", "===", "!==", "<", ">", "<=", ">=", "<<", ">>", "&&", "||", "&", "|", "=",
    "+=", "-=", "*=", "/=", "%=",
];
const POSTFIX_OPERATORS: &[&str] = &[".", "(", "[", "?"];
/// Tokens that start an expression; several of them together read as "an expression".
const EXPRESSION_STARTS: &[&str] = &[
    "(", "{", "[", "!", "\"", "-", "'", "0x", "$", "@", "()", "```", "match", "spawn", "move", "try", "code", "clif",
    "true", "false", "range",
];
/// Built-in type names; several of them together read as "a type".
const TYPE_NAMES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "char", "string", "unit", "never",
    "pointer", "word", "This",
];
/// Punctuation shown first, in this order.
const PUNCTUATION_ORDER: &[&str] = &[";", ",", ")", "]", "}", "(", "{", "[", "=>", "=", ":", "::", ".", "<", ">"];

/// Group and order the expectations into one phrase.
fn render_expected(expected: &[Expectation], missing_semicolon_shape: bool) -> String {
    let wants_pattern =
        expected.iter().any(|expectation| matches!(expectation, Expectation::Thing(t) if t == "a pattern"));
    // `_` and literal suffixes (`_i32`) are characters inside a name or number, not choices.
    let tokens: Vec<&str> = expected
        .iter()
        .filter_map(|expectation| match expectation {
            Expectation::Token(token) if token.starts_with('_') && !(token == "_" && wants_pattern) => None,
            Expectation::Token(token) => Some(token.as_str()),
            _ => None,
        })
        .collect();
    // A token on a new line after a complete expression: the statement just lacks its `;`.
    if missing_semicolon_shape && tokens.contains(&";") {
        return "`;`".to_string();
    }
    let mut things: Vec<String> = expected
        .iter()
        .filter_map(|expectation| match expectation {
            Expectation::Thing(text) => Some(text.clone()),
            _ => None,
        })
        .collect();

    let statement_starts = tokens.iter().filter(|token| STATEMENT_KEYWORDS.contains(token)).count();
    let item_starts = tokens.iter().filter(|token| ITEM_KEYWORDS.contains(token)).count();
    let binary = tokens.iter().filter(|token| BINARY_OPERATORS.contains(token)).count();
    let postfix = tokens.iter().filter(|token| POSTFIX_OPERATORS.contains(token)).count();

    // After a complete expression, `-` and `(` continue it as operators; they start nothing new.
    let operator_like =
        |token: &&str| binary >= 3 && (BINARY_OPERATORS.contains(token) || POSTFIX_OPERATORS.contains(token));
    let expression_starts =
        tokens.iter().filter(|token| EXPRESSION_STARTS.contains(token) && !operator_like(token)).count();
    let type_names = tokens.iter().filter(|token| TYPE_NAMES.contains(token)).count();

    let mut shown: Vec<&str> = Vec::new();
    let collapse_expressions = expression_starts >= 3;
    let collapse_types = type_names >= 2;
    let collapse_statements = statement_starts >= 3;
    let collapse_items = item_starts >= 3;
    let collapse_operators = binary >= 3;
    for token in &tokens {
        if collapse_expressions && EXPRESSION_STARTS.contains(token) && !operator_like(token) {
            continue;
        }
        if collapse_types && TYPE_NAMES.contains(token) {
            continue;
        }
        if collapse_statements && STATEMENT_KEYWORDS.contains(token) {
            continue;
        }
        if collapse_items && ITEM_KEYWORDS.contains(token) {
            continue;
        }
        if collapse_operators
            && (BINARY_OPERATORS.contains(token) || (postfix >= 3 && POSTFIX_OPERATORS.contains(token)))
        {
            continue;
        }
        shown.push(token);
    }
    shown.sort_by_key(|token| PUNCTUATION_ORDER.iter().position(|p| p == token).unwrap_or(PUNCTUATION_ORDER.len()));

    let mut phrases: Vec<String> = shown.iter().map(|token| format!("`{token}`")).collect();
    if collapse_statements {
        things.retain(|thing| thing != "an expression");
        things.insert(0, "a statement".to_string());
    }
    if collapse_items {
        things.insert(0, "an item".to_string());
    }
    if collapse_types {
        things.insert(0, "a type".to_string());
    }
    if collapse_expressions && !collapse_statements {
        things.insert(0, "an expression".to_string());
    }
    if collapse_operators {
        things.retain(|thing| thing != "an assignment operator");
        things.push("an operator".to_string());
    }
    if phrases.iter().any(|phrase| phrase == "`{`") {
        things.retain(|thing| thing != "a block");
    }
    for thing in things {
        if !phrases.contains(&thing) {
            phrases.push(thing);
        }
    }
    if phrases.is_empty() {
        let constructs: Vec<String> = expected
            .iter()
            .filter_map(|expectation| match expectation {
                Expectation::Construct(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        phrases = constructs;
    }
    phrases.dedup();
    if phrases.len() > 5 {
        phrases.truncate(5);
        phrases.push("other syntax".to_string());
    }
    join_alternatives(&phrases)
}

/// "a", "a or b", "a, b or c".
pub fn join_alternatives(phrases: &[String]) -> String {
    match phrases {
        [] => "valid syntax".to_string(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(list: &[&str]) -> Vec<Expectation> {
        list.iter().map(|token| Expectation::Token((*token).to_string())).collect()
    }

    #[test]
    fn joins_alternatives_in_house_style() {
        assert_eq!(join_alternatives(&["`;`".into()]), "`;`");
        assert_eq!(join_alternatives(&["`;`".into(), "`,`".into()]), "`;` or `,`");
        assert_eq!(join_alternatives(&["`;`".into(), "`,`".into(), "an operator".into()]), "`;`, `,` or an operator");
    }

    #[test]
    fn collapses_operator_runs() {
        let mut expected = tokens(&[";", "+", "-", "*", "==", ".", "(", "[", "?"]);
        expected.sort();
        assert_eq!(render_expected(&expected, false), "`;` or an operator");
        assert_eq!(render_expected(&expected, true), "`;`");
    }

    #[test]
    fn drops_trivia_tokens() {
        assert!(is_trivia_token(" "));
        assert!(is_trivia_token("//"));
        assert!(is_trivia_token("a..z"));
        assert!(!is_trivia_token(";"));
        assert!(!is_trivia_token(".."));
    }
}
