//! The one formatting function: source text in, formatted text out.
//!
//! `beskid fmt` (write and `--check`) and the language server's `textDocument/formatting` both
//! call [`format_source`], so the editor and the CLI always agree. Formatting is syntax-only: it
//! reads nothing but the given text.

use crate::analysis::diagnostic_codes::{internal as internal_codes, parse as parse_codes};
use crate::analysis::diagnostics::{SemanticDiagnostic, Severity, make_diagnostic};
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::naming_normalize::normalize_program_naming;
use crate::format::tokens::{self, OpaqueToken};
use crate::format::trivia::{self, Comment};
use crate::parser::{BeskidParser, Rule};
use crate::parsing::parsable::Parsable;
use crate::services::{parse_error_diagnostic, pest_error_diagnostic};
use crate::syntax::{Program, SpanInfo, Spanned};
use pest::Parser;
use std::fmt;

/// Why a source could not be formatted. The source is left unchanged in every case.
#[derive(Debug)]
pub enum FormatError {
    /// The source does not parse. The formatter never repairs or guesses.
    Parse(Box<SemanticDiagnostic>),
    /// The formatter failed or produced output that would lose or change source text (a bug).
    Internal(Box<SemanticDiagnostic>),
}

impl FormatError {
    pub fn diagnostic(&self) -> &SemanticDiagnostic {
        match self {
            FormatError::Parse(d) | FormatError::Internal(d) => d,
        }
    }

    pub fn into_diagnostic(self) -> SemanticDiagnostic {
        match self {
            FormatError::Parse(d) | FormatError::Internal(d) => *d,
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Parse(d) => write!(f, "cannot format a source that does not parse: {}", d.message),
            FormatError::Internal(d) => write!(f, "{}", d.message),
        }
    }
}

impl std::error::Error for FormatError {}

/// A strictly parsed source with its comments.
struct ParsedSource {
    program: Spanned<Program>,
    comments: Vec<Comment>,
    doc_texts: Vec<String>,
    opaque: Vec<OpaqueToken>,
}

impl ParsedSource {
    fn string_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> =
            self.opaque.iter().filter(|t| t.rule == Rule::StringLiteral).map(|t| (t.start, t.end)).collect();
        ranges.sort_unstable();
        ranges
    }
}

/// Parse without recovery: a source with a syntax error is reported, never repaired.
fn parse_strict(source_name: &str, source: &str) -> Result<ParsedSource, FormatError> {
    let mut pairs = BeskidParser::parse(Rule::Program, source)
        .map_err(|err| FormatError::Parse(Box::new(pest_error_diagnostic(source_name, source, &err))))?;
    let pair = pairs.next().ok_or_else(|| {
        FormatError::Parse(Box::new(whole_file_diagnostic(source_name, source, parse_codes::EMPTY_SOURCE, "no program found in source")))
    })?;
    let comments = trivia::collect_comments(source, pair.clone().into_inner());
    let mut opaque = Vec::new();
    tokens::opaque_tokens(pair.clone().into_inner(), &mut opaque);
    opaque.sort_by_key(|t| t.start);
    let doc_texts = pair
        .clone()
        .into_inner()
        .flatten()
        .filter(|p| p.as_rule() == Rule::DocRun)
        .flat_map(|p| p.as_str().lines().map(trivia::comparable).collect::<Vec<_>>())
        .filter(|line| !line.is_empty())
        .collect();
    let program = crate::syntax::common::span::with_line_index(source, || Program::parse(pair))
        .map_err(|err| FormatError::Parse(Box::new(parse_error_diagnostic(source_name, source, &err))))?;
    Ok(ParsedSource { program, comments, doc_texts, opaque })
}

fn whole_file_diagnostic(
    source_name: &str,
    source: &str,
    code: &str,
    message: impl Into<String>,
) -> SemanticDiagnostic {
    let end = if source.is_empty() { 0 } else { 1.min(source.len()) };
    make_diagnostic(
        source_name,
        source,
        SpanInfo { start: 0, end, line_col_start: (1, 1), line_col_end: (1, 1) },
        message,
        "format",
        None,
        Some(code.to_string()),
        Severity::Error,
    )
}

fn internal(source_name: &str, source: &str, message: impl Into<String>) -> FormatError {
    FormatError::Internal(Box::new(whole_file_diagnostic(source_name, source, internal_codes::FORMATTER_FAILURE, message)))
}

/// Format `source` in the house style.
///
/// Fails closed: a source that does not parse is an error (nothing is repaired), and the output
/// is checked before it is returned. It must parse, have the same code tokens apart from the
/// ruled rewrites (see `tokens.rs`), keep every comment and every documentation line in order,
/// and format to itself again; otherwise the result is an internal error and the caller keeps
/// the original text.
pub fn format_source(source_name: &str, source: &str) -> Result<String, FormatError> {
    let parsed = parse_strict(source_name, source)?;
    let strings = parsed.string_ranges();
    let code_before = tokens::code_tokens(source, &parsed.comments, &parsed.opaque);
    let formatted = emit(source, parsed.program, parsed.comments.clone(), strings)
        .map_err(|err| internal(source_name, source, format!("format error: {err}")))?;

    let reparsed = parse_strict(source_name, &formatted).map_err(|err| {
        internal(source_name, source, format!("formatter produced text that does not parse ({err}); file left unchanged"))
    })?;
    let before: Vec<String> = parsed.comments.iter().map(|c| trivia::comparable(&c.text)).collect();
    let after: Vec<String> = reparsed.comments.iter().map(|c| trivia::comparable(&c.text)).collect();
    if before != after {
        return Err(internal(
            source_name,
            source,
            "formatter would drop or reorder a comment; file left unchanged",
        ));
    }
    let code_after = tokens::code_tokens(&formatted, &reparsed.comments, &reparsed.opaque);
    if let Some(difference) = tokens::first_difference(&code_before, &code_after) {
        return Err(internal(
            source_name,
            source,
            format!("formatter would change the code, not only its layout ({difference}); file left unchanged"),
        ));
    }
    if parsed.doc_texts != reparsed.doc_texts {
        return Err(internal(
            source_name,
            source,
            "formatter would drop or change a documentation comment; file left unchanged",
        ));
    }
    let strings = reparsed.string_ranges();
    let again = emit(&formatted, reparsed.program, reparsed.comments, strings)
        .map_err(|err| internal(source_name, source, format!("format error: {err}")))?;
    if again != formatted {
        return Err(internal(source_name, source, "formatter output is not stable; file left unchanged"));
    }
    Ok(formatted)
}

/// Print a program that has no source text, such as items a Mod generated.
///
/// Source text, and so comments, do not exist for such a program. Never use this for a parsed
/// file: [`format_source`] is the only function that keeps a file's comments.
pub fn format_generated_program(program: &Spanned<Program>) -> Result<String, EmitError> {
    let mut program = program.clone();
    normalize_program_naming(&mut program.node);
    let mut cx = EmitCtx::new();
    let mut out = String::new();
    program.node.emit(&mut out, &mut cx)?;
    Ok(out)
}

fn emit(
    source: &str,
    mut program: Spanned<Program>,
    comments: Vec<Comment>,
    strings: Vec<(usize, usize)>,
) -> Result<String, EmitError> {
    normalize_program_naming(&mut program.node);
    let mut cx = EmitCtx::with_source(source, comments, strings);
    let mut out = String::new();
    program.node.emit(&mut out, &mut cx)?;
    Ok(out)
}
