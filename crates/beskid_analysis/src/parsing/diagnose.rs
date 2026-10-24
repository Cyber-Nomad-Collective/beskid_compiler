//! Parse failures as structured diagnostics (codes `E0101`–`E0199`).
//!
//! The message says what the parser expected, what it was parsing and what it found:
//! "expected `(` while parsing host `App`, found `{`". The span is the token found; the start of the
//! construct is a related location. Known mistakes ([`detect_mistake`]) refine the message, carry one
//! help line and, where the repair has one meaning, a machine-applicable fix. A fix is offered only
//! when applying it lets the parser get past the mistake.

use crate::analysis::diagnostics::{SemanticDiagnostic, Severity, make_diagnostic};
use crate::analysis::{Applicability, Fix, FixEdit};
use crate::parser::Rule;
use crate::syntax::SpanInfo;

use super::failure::{ParseFailure, detailed_error};
use super::syntax_context::{
    Construct, FrameKind, RESERVED_WORDS, Tok, TokKind, item_head, previous_token_end, skip_trivia, token_at,
    tokens_before,
};

/// Parse error codes. The registry entries live in `analysis::diagnostic_codes::parse`.
pub mod codes {
    pub const SYNTAX: &str = "E0101";
    pub const MISSING_SEMICOLON: &str = "E0102";
    pub const UNCLOSED_DELIMITER: &str = "E0103";
    pub const HOST_WITHOUT_PARAMETERS: &str = "E0104";
    pub const INCREMENT_OPERATOR: &str = "E0105";
    pub const TYPED_LET: &str = "E0106";
    pub const RESERVED_WORD: &str = "E0107";
    pub const BLOCK_MATCH_ARM: &str = "E0108";
    pub const UNKNOWN_ESCAPE: &str = "E0109";
    pub const INVALID_UNICODE_ESCAPE: &str = "E0110";
    pub const UNEXPECTED_TREE: &str = "E0111";
    pub const SELF_PARAMETER: &str = "E0112";
    pub const EMPTY_SOURCE: &str = "E0113";
    pub const LONG_CHARACTER_LITERAL: &str = "E0114";
}

/// A parse failure as a diagnostic, plus the repaired source when the fix makes it parse.
#[derive(Debug, Clone)]
pub struct ParseDiagnosis {
    pub diagnostic: SemanticDiagnostic,
    /// The source with the fix applied, when that source parses as the same rule.
    pub repaired_source: Option<String>,
    /// The diagnostic names a specific mistake (its own code), not only the expected tokens.
    pub specific: bool,
    /// The construct being parsed, in words.
    construct: Option<String>,
    /// The token found, in words.
    found: String,
}

/// A recognised mistake: how it refines the generic diagnostic.
struct Mistake {
    code: &'static str,
    message: Option<String>,
    expected: Option<String>,
    span: Option<(usize, usize)>,
    label: Option<String>,
    help: String,
    related: Vec<(usize, usize, String)>,
    fix: Option<(String, Vec<FixEdit>)>,
    /// How safe the fix is; most mistake fixes have one meaning.
    applicability: Applicability,
}

impl Mistake {
    fn new(code: &'static str, help: impl Into<String>) -> Self {
        Self {
            code,
            message: None,
            expected: None,
            span: None,
            label: None,
            help: help.into(),
            related: Vec::new(),
            fix: None,
            applicability: Applicability::MachineApplicable,
        }
    }
}

/// Diagnose the failure of parsing `source` as `rule`.
pub fn diagnose_parse_failure(
    source_name: &str,
    source: &str,
    rule: Rule,
    error: &pest::error::Error<Rule>,
) -> ParseDiagnosis {
    let failure = ParseFailure::analyze(rule, source, error);
    let construct = failure.context.construct();
    let mistake = detect_mistake(source, &failure, construct.as_ref());

    let expected =
        mistake.as_ref().and_then(|mistake| mistake.expected.clone()).unwrap_or_else(|| failure.expected_words(source));
    let found = failure.found_words(source);
    let message = mistake.as_ref().and_then(|mistake| mistake.message.clone()).unwrap_or_else(|| match &construct {
        Some(construct) => format!("expected {expected} while parsing {}, found {found}", construct.description),
        None => format!("expected {expected}, found {found}"),
    });
    let (start, end) = mistake
        .as_ref()
        .and_then(|mistake| mistake.span)
        .unwrap_or_else(|| failure.found.as_ref().map_or((source.len(), source.len()), |tok| (tok.start, tok.end)));
    let label =
        mistake.as_ref().and_then(|mistake| mistake.label.clone()).unwrap_or_else(|| format!("expected {expected}"));
    let help = match &mistake {
        Some(mistake) => Some(mistake.help.clone()),
        None => foreign_syntax_help(failure.found.as_ref())
            .or_else(|| conditional_operator_help(source, &failure))
            .or_else(|| foreign_item_help(&failure))
            .or_else(|| construct.as_ref().and_then(|construct| construct_help(&construct.description))),
    };
    let code = mistake.as_ref().map_or(codes::SYNTAX, |mistake| mistake.code);

    let mut diagnostic = make_diagnostic(
        source_name,
        source,
        SpanInfo::from_byte_range_in_source(source, start, end),
        message,
        label,
        help,
        Some(code.to_string()),
        Severity::Error,
    );
    if let Some(construct) = &construct
        && construct.start < start
    {
        diagnostic = diagnostic.with_related(
            SpanInfo::from_byte_range_in_source(source, construct.start, token_end(source, construct.start)),
            format!("{} starts here", construct.description),
        );
    }
    let mut repaired_source = None;
    let specific = mistake.is_some();
    let construct_words = construct.as_ref().map(|construct| construct.description.clone());
    if let Some(mistake) = mistake {
        for (start, end, label) in mistake.related {
            diagnostic = diagnostic.with_related(SpanInfo::from_byte_range_in_source(source, start, end), label);
        }
        let applicability = mistake.applicability;
        if let Some((title, edits)) = mistake.fix {
            match check_fix(rule, source, &edits, failure.position) {
                FixCheck::Parses(repaired) => {
                    repaired_source = Some(repaired);
                    diagnostic = diagnostic.with_fix(Fix::new(title, applicability, edits));
                }
                FixCheck::GetsPast => {
                    diagnostic = diagnostic.with_fix(Fix::new(title, applicability, edits));
                }
                FixCheck::Fails => {}
            }
        }
    }
    ParseDiagnosis { diagnostic, repaired_source, specific, construct: construct_words, found }
}

/// What the grammar engine's repair means for the user, when it has one meaning.
pub enum EngineRepair<'a> {
    /// The engine inserted `text` at `position`.
    Insert { position: usize, text: &'a str },
    /// Any other repair: reported without a fix.
    Other,
}

/// Refine a generic diagnostic with a repair the recovery engine found (and that parses).
///
/// Only a missing `;` and missing closing delimiters have one meaning; other repairs leave the
/// diagnostic as it is.
pub fn refine_with_engine_repair(
    diagnosis: ParseDiagnosis,
    source: &str,
    rule: Rule,
    repair: EngineRepair<'_>,
) -> ParseDiagnosis {
    if diagnosis.specific || !diagnosis.diagnostic.fixes.is_empty() {
        return diagnosis;
    }
    let EngineRepair::Insert { position, text } = repair else {
        return diagnosis;
    };
    let trimmed = text.trim();
    let (code, title, expected, help) = if trimmed == ";" {
        (
            codes::MISSING_SEMICOLON,
            "Insert `;`".to_string(),
            "`;`".to_string(),
            "end the statement with `;`".to_string(),
        )
    } else if !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, ')' | ']' | '}' | ' ')) {
        let closers: String = trimmed.chars().filter(|ch| !ch.is_whitespace()).collect();
        let words = closers.chars().map(|ch| format!("`{ch}`")).collect::<Vec<_>>().join(" and ");
        (codes::UNCLOSED_DELIMITER, format!("Insert {words}"), words, "close every opened delimiter".to_string())
    } else {
        return diagnosis;
    };
    // A `;` belongs right after the token it ends, not before the next line's token.
    let mut edits = vec![FixEdit::insert(position, text.to_string())];
    if trimmed == ";"
        && let Some(end) = previous_token_end(source, position)
        && end < position
    {
        let moved = vec![FixEdit::insert(end, ";")];
        if matches!(check_fix(rule, source, &moved, position), FixCheck::Parses(_)) {
            edits = moved;
        }
    }
    let ParseDiagnosis { mut diagnostic, repaired_source, construct, found, .. } = diagnosis;
    diagnostic.message = match &construct {
        Some(construct) => format!("expected {expected} while parsing {construct}, found {found}"),
        None => format!("expected {expected}, found {found}"),
    };
    diagnostic.label = format!("expected {expected}");
    diagnostic.code = Some(code.to_string());
    diagnostic.help = Some(help);
    ParseDiagnosis {
        diagnostic: diagnostic.with_fix(Fix::new(title, Applicability::MachineApplicable, edits)),
        repaired_source,
        specific: true,
        construct,
        found,
    }
}

enum FixCheck {
    /// The fixed source parses.
    Parses(String),
    /// The fixed source still fails, but after the mistake.
    GetsPast,
    Fails,
}

fn apply_edits(source: &str, edits: &[FixEdit]) -> Option<String> {
    let mut sorted: Vec<&FixEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| edit.range().0);
    let mut out = String::with_capacity(source.len() + 16);
    let mut cursor = 0usize;
    for edit in sorted {
        let (start, end) = edit.range();
        if start < cursor || end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return None;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&edit.replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    Some(out)
}

fn check_fix(rule: Rule, source: &str, edits: &[FixEdit], failure_position: usize) -> FixCheck {
    let Some(fixed) = apply_edits(source, edits) else {
        return FixCheck::Fails;
    };
    let Some(error) = detailed_error(rule, &fixed) else {
        return FixCheck::Parses(fixed);
    };
    let delta: isize = edits
        .iter()
        .filter(|edit| edit.range().0 <= failure_position)
        .map(|edit| edit.replacement.len() as isize - (edit.range().1 - edit.range().0) as isize)
        .sum();
    let moved_to = failure_position as isize + delta;
    let reached = error.parse_attempts().map_or(0, |attempts| attempts.max_position) as isize;
    if reached > moved_to { FixCheck::GetsPast } else { FixCheck::Fails }
}

fn token_end(source: &str, start: usize) -> usize {
    token_at(source, start).map_or(start + 1, |tok| tok.end)
}

/// Reserved words that legitimately start a statement or expression.
const STATEMENT_WORDS: &[&str] = &[
    "if", "else", "while", "for", "return", "break", "continue", "let", "mut", "move", "match", "use", "with",
    "launch", "try", "spawn", "clif", "in", "when", "where", "catch",
];
/// Reserved words that legitimately start an item or member.
const ITEM_WORDS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "attribute",
    "impl",
    "extend",
    "mod",
    "use",
    "pub",
    "test",
    "host",
    "const",
    "registry",
    "scope",
    "startup",
    "init",
    "dispose",
    "inject",
    "single",
    "transient",
    "where",
    "mut",
    "move",
    "bulk",
    "skip",
];

fn detect_mistake(source: &str, failure: &ParseFailure, construct: Option<&Construct>) -> Option<Mistake> {
    reserved_word(source, failure)
        .or_else(|| long_character_literal(source, failure))
        .or_else(|| host_without_parameters(failure))
        .or_else(|| increment_operator(source, failure))
        .or_else(|| typed_let(source, failure))
        .or_else(|| block_match_arm(failure))
        .or_else(|| missing_semicolon(source, failure, construct))
        .or_else(|| unclosed_at_end(source, failure))
}

fn reserved_word(source: &str, failure: &ParseFailure) -> Option<Mistake> {
    let found = failure.found.as_ref()?;
    let word = found.word()?;
    if !RESERVED_WORDS.contains(&word) || !failure.expects_thing("a name") {
        return None;
    }
    let before = tokens_before(source, found.start);
    let previous = before.last()?;
    let certainly_a_name = previous.is_word("let") || previous.is_word("mut");
    if !certainly_a_name {
        let plausible = previous.word().is_some() || previous.is_punct(">") || previous.is_punct("]");
        let excluded = match failure.context.innermost().kind {
            FrameKind::Code | FrameKind::Match | FrameKind::Paren | FrameKind::Bracket | FrameKind::Braces => {
                STATEMENT_WORDS.contains(&word)
            }
            _ => ITEM_WORDS.contains(&word),
        };
        if !plausible || excluded {
            return None;
        }
    }
    let mut mistake =
        Mistake::new(codes::RESERVED_WORD, "reserved words cannot name bindings, parameters, fields or items");
    mistake.message = Some(format!("`{word}` is a reserved word; choose another name"));
    mistake.label = Some("reserved word".to_string());
    Some(mistake)
}

/// `'hello'`: single quotes around more than one character, meant as a string.
fn long_character_literal(source: &str, failure: &ParseFailure) -> Option<Mistake> {
    let position = failure.position.min(source.len());
    let line_start = source[..position].rfind('\n').map_or(0, |index| index + 1);
    let open = line_start + source[line_start..position].rfind('\'')?;
    let line_end = source[open..].find('\n').map_or(source.len(), |index| open + index);
    let close = open + 1 + source[open + 1..line_end].find('\'')?;
    let text = &source[open + 1..close];
    // One character or one escape is a valid character literal, not this mistake.
    if close < position || text.chars().count() < 2 || text.starts_with('\\') {
        return None;
    }
    let escaped = text.replace('"', "\\\"");
    let replacement = format!("\"{escaped}\"");
    let mut mistake = Mistake::new(
        codes::LONG_CHARACTER_LITERAL,
        format!("single quotes hold one character; for text, write the string `{replacement}`"),
    );
    mistake.message = Some("a character literal holds exactly one character".to_string());
    mistake.span = Some((open, close + 1));
    mistake.label = Some("more than one character".to_string());
    mistake.fix = Some((
        format!("Write the string `{replacement}`"),
        vec![FixEdit::new(SpanInfo::from_byte_range_in_source(source, open, close + 1), replacement)],
    ));
    // A string has another type than a character; the surrounding code may need changes too.
    mistake.applicability = Applicability::MaybeIncorrect;
    Some(mistake)
}

fn host_without_parameters(failure: &ParseFailure) -> Option<Mistake> {
    let found = failure.found.as_ref()?;
    if !(found.is_punct("{") || found.is_punct(":")) {
        return None;
    }
    let frame = failure.context.innermost();
    if !matches!(frame.kind, FrameKind::Items) {
        return None;
    }
    let head = item_head(failure.context.segment());
    let [host, name] = head else {
        return None;
    };
    if !host.is_word("host") {
        return None;
    }
    let name_text = name.word()?;
    let mut mistake = Mistake::new(
        codes::HOST_WITHOUT_PARAMETERS,
        format!("a host takes a parameter list, even an empty one: `host {name_text}() {{`"),
    );
    mistake.expected = Some("`(`".to_string());
    mistake.fix = Some(("Insert `()`".to_string(), vec![FixEdit::insert(name.end, "()")]));
    Some(mistake)
}

fn increment_operator(source: &str, failure: &ParseFailure) -> Option<Mistake> {
    let limit = failure.found.as_ref().map_or(source.len(), |tok| tok.start + 1);
    let before = tokens_before(source, limit);
    // `i++` stops inside the `++` token; `i--` stops at the token after it.
    let last = before.len().checked_sub(1)?;
    let op_index = if before[last].is_punct("++") || before[last].is_punct("--") {
        last
    } else {
        let index = last.checked_sub(1)?;
        if !(before[index].is_punct("++") || before[index].is_punct("--")) {
            return None;
        }
        index
    };
    let op = &before[op_index];
    let operand = before.get(op_index.checked_sub(1)?)?;
    if !(operand.word().is_some() || operand.is_punct(")") || operand.is_punct("]")) {
        return None;
    }
    let after = token_at(source, op.end);
    if !after.as_ref().is_none_or(|tok| tok.is_punct(";") || tok.is_punct(")")) {
        return None;
    }
    let (symbol, replacement) = if op.is_punct("++") { ("++", "+= 1") } else { ("--", "-= 1") };
    let operand_text = &source[operand.start..operand.end];
    let mut mistake = Mistake::new(codes::INCREMENT_OPERATOR, format!("write `{operand_text} {replacement}`"));
    mistake.message = Some(format!("`{symbol}` is not a Beskid operator"));
    mistake.span = Some((op.start, op.end));
    mistake.label = Some("not an operator".to_string());
    mistake.fix = Some((
        format!("Replace `{symbol}` with `{replacement}`"),
        vec![FixEdit::new(SpanInfo::from_byte_range_in_source(source, operand.end, op.end), format!(" {replacement}"))],
    ));
    Some(mistake)
}

fn typed_let(source: &str, failure: &ParseFailure) -> Option<Mistake> {
    let found = failure.found.as_ref()?;
    if !found.is_punct(":") || failure.context.innermost().kind != FrameKind::Code {
        return None;
    }
    let segment = failure.context.segment();
    let (let_tok, mutable, name) = match segment {
        [let_tok, name] if let_tok.is_word("let") => (let_tok, false, name),
        [let_tok, mut_tok, name] if let_tok.is_word("let") && mut_tok.is_word("mut") => (let_tok, true, name),
        _ => return None,
    };
    let name_text = name.word()?;
    // The type runs from after `:` to the `=` or `;` at nesting depth zero.
    let type_start = skip_trivia(source, found.end);
    let mut depth = 0isize;
    let mut cursor = type_start;
    let mut type_end = type_start;
    while let Some(tok) = token_at(source, cursor) {
        match &tok.kind {
            TokKind::Punct("<" | "(" | "[") => depth += 1,
            TokKind::Punct(">" | ")" | "]") => depth -= 1,
            TokKind::Punct(">>") => depth -= 2,
            TokKind::Punct("=" | ";" | "{" | "}") if depth <= 0 => break,
            _ => {}
        }
        type_end = tok.end;
        cursor = tok.end;
    }
    if type_end <= type_start {
        return None;
    }
    let type_text = source[type_start..type_end].trim();
    let replacement = format!("{}{type_text} {name_text}", if mutable { "mut " } else { "" });
    let mut mistake = Mistake::new(
        codes::TYPED_LET,
        format!("`let` infers the type; to state it, write the type first: `{replacement}`"),
    );
    mistake.expected = Some("`=`".to_string());
    mistake.fix = Some((
        format!("Write `{replacement}`"),
        vec![FixEdit::new(SpanInfo::from_byte_range_in_source(source, let_tok.start, type_end), replacement)],
    ));
    Some(mistake)
}

fn block_match_arm(failure: &ParseFailure) -> Option<Mistake> {
    let frame = failure.context.innermost();
    if !frame.arm_block || !failure.found.as_ref().is_some_and(|tok| tok.is_punct("}")) || !failure.expects_token(";") {
        return None;
    }
    let (_, open) = frame.opener?;
    let mut mistake = Mistake::new(
        codes::BLOCK_MATCH_ARM,
        "move the block into a function and call it from the arm, for example `1 => Compute(x),`",
    );
    mistake.message = Some("match arms cannot be blocks yet; call a function or use `if`".to_string());
    mistake.span = Some((open, open + 1));
    mistake.label = Some("block used as the arm's value".to_string());
    Some(mistake)
}

fn missing_semicolon(source: &str, failure: &ParseFailure, construct: Option<&Construct>) -> Option<Mistake> {
    if !failure.expects_token(";") {
        return None;
    }
    let found = failure.found.as_ref();
    let shape = failure.found_on_new_line(source) || found.is_none() || found.is_some_and(|tok| tok.is_punct("}"));
    if !shape {
        return None;
    }
    let end = previous_token_end(source, failure.position)?;
    let what = construct.map_or("statement", |construct| {
        if construct.description.contains("declaration") { "declaration" } else { "statement" }
    });
    let mut mistake = Mistake::new(codes::MISSING_SEMICOLON, format!("end the {what} with `;`"));
    mistake.expected = Some("`;`".to_string());
    mistake.fix = Some(("Insert `;`".to_string(), vec![FixEdit::insert(end, ";")]));
    Some(mistake)
}

fn unclosed_at_end(source: &str, failure: &ParseFailure) -> Option<Mistake> {
    if failure.found.is_some() {
        return None;
    }
    let open: Vec<_> =
        failure.context.frames.iter().filter_map(|frame| frame.opener.map(|opener| (frame, opener))).collect();
    let (innermost, (open_char, open_at)) = *open.last()?;
    let closers: String = open.iter().rev().filter_map(|(frame, _)| frame.closer()).collect();
    let text = if closers.starts_with('}') {
        if source.ends_with('\n') { format!("{closers}\n") } else { format!("\n{closers}") }
    } else {
        closers.clone()
    };
    let first = innermost.closer()?;
    let mut mistake = Mistake::new(codes::UNCLOSED_DELIMITER, format!("add `{first}` to close the `{open_char}`"));
    mistake.expected = Some(format!("`{first}`"));
    mistake.related.push((open_at, open_at + 1, format!("unclosed `{open_char}`")));
    let words = closers.chars().map(|ch| format!("`{ch}`")).collect::<Vec<_>>().join(" and ");
    mistake.fix = Some((format!("Insert {words}"), vec![FixEdit::insert(source.len(), text)]));
    Some(mistake)
}

/// `x > 0 ? 1 : 2`: `?` parses as the error-propagation operator, so the parse fails one token later.
fn conditional_operator_help(source: &str, failure: &ParseFailure) -> Option<String> {
    let found = failure.found.as_ref()?;
    if found.is_punct(";") || found.is_punct(")") || found.is_punct(",") || found.is_punct("}") {
        return None;
    }
    let previous = tokens_before(source, found.start).pop()?;
    previous.is_punct("?").then(|| "Beskid has no `? :` operator; use `match` or an `if` block".to_string())
}

/// Help when the item or statement being parsed starts with another language's keyword
/// (`fn Add(a: i32)` fails later, at the `:`).
fn foreign_item_help(failure: &ParseFailure) -> Option<String> {
    failure.context.frames.iter().rev().find_map(|frame| {
        let first = item_head(&frame.segment).first()?;
        first.word()?;
        if first.word().is_some_and(|word| RESERVED_WORDS.contains(&word)) {
            return None;
        }
        foreign_syntax_help(Some(first))
    })
}

/// Help for a token that belongs to another language's syntax.
fn foreign_syntax_help(found: Option<&Tok>) -> Option<String> {
    let found = found?;
    let help = match &found.kind {
        TokKind::Word(word) => match word.as_str() {
            "fn" | "func" | "function" | "def" => {
                "a function starts with its return type: `i32 Add(i32 a, i32 b) { ... }`"
            }
            "elif" | "elsif" => "write `else if`",
            "switch" => "use `match value { pattern => result, _ => fallback }`",
            "foreach" => "write `for item in items { ... }`",
            "struct" | "class" | "record" => "declare a type with `type Name { i32 X, i32 Y }`",
            "interface" | "trait" | "protocol" => "declare a contract with `contract Name { ... }`",
            "import" | "using" => "import a module with `use Module.Name;`",
            "namespace" | "package" => "declare a module with `mod Name;`",
            "public" | "private" | "export" => "items are private by default; write `pub` to export one",
            "static" => "Beskid has no `static`; declare the function at module level",
            "and" | "or" | "not" => "use `&&`, `||` and `!`",
            "var" | "val" => "write `let name = value;` or `Type name = value;`",
            _ => return None,
        },
        TokKind::Punct("->") => "a return type comes first: `i32 Name()`; match arms and lambdas use `=>`",
        TokKind::Punct("?") => "Beskid has no `? :` operator; use `match` or an `if` block",
        TokKind::Punct("'") => "strings use double quotes: `\"text\"`; single quotes hold one character",
        TokKind::Punct("$") => "interpolate with `\"${value}\"`",
        TokKind::Punct("++") => "write `+= 1`",
        TokKind::Punct("--") => "write `-= 1`",
        TokKind::Other('#') => "comments start with `//`",
        _ => return None,
    };
    Some(help.to_string())
}

/// Help that shows the shape of the construct being parsed.
fn construct_help(description: &str) -> Option<String> {
    let first = description.split(' ').next().unwrap_or("");
    let help = match first {
        "host" => "a host is written `host Name(parameters) { ... }`",
        "function" => "a function is written `ReturnType Name(Type name, ...) { ... }`",
        "type" | "field" => "fields are written `Type Name` and separated by commas: `type Point { i32 X, i32 Y }`",
        "method" => "a method is written `ReturnType Name(Type name, ...) { ... }`",
        "enum" | "variant" => "variants are separated by commas: `enum Shape { Circle(f64 radius), Empty }`",
        "contract" => "a contract lists method signatures such as `i32 Size();`",
        "constant" => "a constant is written `const Name = 10;`",
        "test" => "a test is written `test Name { ... }`",
        "module" => "a module is declared with `mod Name;` or `mod Name { ... }`",
        "`use`" => "a `use` declaration is written `use Module.Name;`",
        "`let`" => "write `let name = value;`, or `Type name = value;` to state the type",
        "binding" => "a binding is written `Type name = value;` or `let name = value;`",
        "`if`" => "`if` takes a condition and a block: `if x > 0 { ... }`",
        "`else`" => "write `else { ... }` or `else if condition { ... }`",
        "`while`" => "`while` takes a condition and a block: `while x < 10 { ... }`",
        "`for`" => "a loop is written `for item in items { ... }` or `for i in range(0, 10) { ... }`",
        "`match`" | "match" => "match arms are written `pattern => expression` and separated by commas",
        "`return`" => "write `return value;` or `return;`",
        "call" | "`range(...)`" => "arguments are separated by commas: `Name(a, b)`",
        "parameters" => "parameters are written `Type name` and separated by commas",
        "statement" => "every statement ends with `;`",
        "attribute" => "an attribute is written `[Name]` or `[Name(key: value)]`",
        _ => return None,
    };
    Some(help.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_help_covers_scanned_constructs() {
        for description in
            ["host `App`", "function `Main`", "`let` binding `x`", "binding `x`", "call to `Foo`", "statement"]
        {
            assert!(construct_help(description).is_some(), "{description}");
        }
    }

    #[test]
    fn applies_edits_in_order() {
        let edits = vec![FixEdit::insert(3, "!"), FixEdit::insert(0, "<")];
        assert_eq!(apply_edits("abcdef", &edits).as_deref(), Some("<abc!def"));
    }
}
