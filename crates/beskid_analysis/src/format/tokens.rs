//! Token-stream self-check: formatting may change layout, never code.
//!
//! The input and the output are reduced to their code tokens (no whitespace, no comments, no
//! documentation; comments and documentation have their own checks) and compared after the
//! rewrites the house style rules allow. Identifiers are compared exactly: the formatter never
//! renames (the naming-style warnings suggest a name; Rename Symbol renames a declaration with its uses).
//! Operators are read longest first (`>>`, `=>`, `::` are one token each). The allowed rewrites:
//! - parentheses around an `if`, `while` or `match` condition: the pair right after the keyword
//!   whose `)` is followed by the body's `{`;
//! - trailing commas before `}`, `)` or `]` (the printer writes none in argument, parameter and
//!   array lists, and one after every member of a braced list), and an optional `;` before `}`
//!   (a contract embedding); repeated commas between match arms collapse to one;
//! - empty `()` at the places the parse recorded (a nullary enum constructor or enum pattern, an
//!   attribute without arguments), in the input only;
//! - parentheses around a single lambda parameter.
//!
//! Anything else that differs (a string interpolation printed as `+` concatenation, a lost
//! operator or call, a changed literal) makes the formatter refuse the file.

use crate::format::trivia::{Comment, is_opaque};
use crate::parser::Rule;
use crate::parsing::syntax_context::PUNCTS;
use pest::iterators::Pair;

/// A token whose text is taken whole (string, character, documentation, code or CLIF literal).
#[derive(Debug, Clone, Copy)]
pub(crate) struct OpaqueToken {
    pub start: usize,
    pub end: usize,
    pub rule: Rule,
}

/// Opaque tokens of a parse tree, in source order.
pub(crate) fn opaque_tokens<'i>(pairs: impl Iterator<Item = Pair<'i, Rule>>, out: &mut Vec<OpaqueToken>) {
    for pair in pairs {
        if is_opaque(pair.as_rule()) {
            let span = pair.as_span();
            out.push(OpaqueToken { start: span.start(), end: span.end(), rule: pair.as_rule() });
        } else {
            opaque_tokens(pair.into_inner(), out);
        }
    }
}

/// Source offsets of the `(` of empty parentheses the printer omits: a nullary enum constructor
/// (`E::None()`), an enum pattern without fields (`E::A()`) and an attribute without arguments
/// (`[Mark()]`). Parentheses that hold a comment are not recorded (the file is then refused).
pub(crate) fn omitted_empty_parens<'i>(pairs: impl Iterator<Item = Pair<'i, Rule>>, out: &mut Vec<usize>) {
    for pair in pairs {
        let (head, list) = match pair.as_rule() {
            Rule::enum_constructor_with_args => (Rule::EnumPath, Rule::ArgumentList),
            Rule::EnumPattern => (Rule::EnumPath, Rule::PatternList),
            Rule::Attribute => (Rule::Identifier, Rule::AttributeArgumentList),
            _ => {
                omitted_empty_parens(pair.into_inner(), out);
                continue;
            }
        };
        let children: Vec<Pair<'i, Rule>> = pair.clone().into_inner().collect();
        if !children.iter().any(|child| child.as_rule() == list)
            && let Some(name) = children.iter().find(|child| child.as_rule() == head)
        {
            let rest = &pair.as_span().get_input()[name.as_span().end()..pair.as_span().end()];
            let open = rest.len() - rest.trim_start().len();
            if let Some(after_open) = rest[open..].strip_prefix('(')
                && after_open.trim_start().starts_with(')')
            {
                out.push(name.as_span().end() + open);
            }
        }
        omitted_empty_parens(pair.into_inner(), out);
    }
}

/// The comparable code tokens of `source`. `omitted_parens` holds the sorted offsets of empty
/// `()` the printer leaves out (see [`omitted_empty_parens`]); pass none for the printer's output.
pub(crate) fn code_tokens(
    source: &str,
    comments: &[Comment],
    opaque: &[OpaqueToken],
    omitted_parens: &[usize],
) -> Vec<String> {
    let mut skips: Vec<(usize, usize, Option<Rule>)> = comments.iter().map(|c| (c.start, c.end, None)).collect();
    skips.extend(opaque.iter().map(|t| (t.start, t.end, Some(t.rule))));
    skips.sort_by_key(|s| s.0);

    let mut tokens: Vec<String> = Vec::new();
    let mut drop_next_close = false;
    let mut i = 0usize;
    let mut next_skip = 0usize;
    while i < source.len() {
        while next_skip < skips.len() && skips[next_skip].1 <= i {
            next_skip += 1;
        }
        if let Some(&(start, end, rule)) = skips.get(next_skip)
            && start <= i
        {
            match rule {
                None | Some(Rule::DocRun) => {}
                // A `code` fence may end with the newline after its closing fence; the printer
                // ends the literal at the fence. Everything else is compared exactly.
                Some(Rule::CodeExpression) => tokens.push(trim_one_newline(&source[start..end]).to_string()),
                Some(_) => tokens.push(source[start..end].to_string()),
            }
            i = end;
            next_skip += 1;
            continue;
        }
        let ch = source[i..].chars().next().expect("char at a char boundary");
        if ch.is_whitespace() {
            i += ch.len_utf8();
        } else if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < source.len() {
                let c = source[i..].chars().next().expect("char");
                if c.is_alphanumeric() || c == '_' {
                    i += c.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(source[start..i].to_string());
        } else if ch == '(' && omitted_parens.binary_search(&i).is_ok() {
            drop_next_close = true;
            i += 1;
        } else if ch == ')' && drop_next_close {
            drop_next_close = false;
            i += 1;
        } else {
            let text =
                PUNCTS.iter().find(|p| source[i..].starts_with(**p)).map_or_else(|| ch.to_string(), |p| p.to_string());
            i += text.len();
            tokens.push(text);
        }
    }
    normalize(tokens)
}

/// `text` without one trailing line ending.
pub(crate) fn trim_one_newline(text: &str) -> &str {
    text.strip_suffix('\n').map_or(text, |t| t.strip_suffix('\r').unwrap_or(t))
}

fn matching_close(tokens: &[String], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Apply the allowed rewrites so equal code gives equal token lists.
fn normalize(mut tokens: Vec<String>) -> Vec<String> {
    // Condition parentheses: the pair right after the keyword that closes just before the body's
    // `{`, removed repeatedly (`if ((a)) {` reduces to `if a {`; in `if (a || b) && c {` the pair
    // groups only part of the condition and stays).
    let mut index = 0;
    while index < tokens.len() {
        if matches!(tokens[index].as_str(), "if" | "while" | "match") {
            while tokens.get(index + 1).is_some_and(|t| t == "(") {
                let Some(close) = matching_close(&tokens, index + 1) else { break };
                if tokens.get(close + 1).is_none_or(|t| t != "{") {
                    break;
                }
                tokens.remove(close);
                tokens.remove(index + 1);
            }
        }
        index += 1;
    }
    // A single lambda parameter: `( x ) =>` becomes `x =>`.
    let mut index = 0;
    while index + 3 < tokens.len() {
        if tokens[index] == "("
            && tokens[index + 2] == ")"
            && tokens[index + 3] == "=>"
            && tokens[index + 1].chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            tokens.remove(index + 2);
            tokens.remove(index);
        }
        index += 1;
    }
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    for token in tokens {
        match token.as_str() {
            // Trailing comma before a closer, and an optional `;` before a closing brace.
            "}" | ")" | "]" if out.last().is_some_and(|t| t == ",") => {
                out.pop();
                out.push(token);
            }
            "}" if out.last().is_some_and(|t| t == ";") => {
                out.pop();
                out.push(token);
            }
            // Repeated commas between match arms (`1 => a,, _ => b`): the printer writes one.
            "," if out.last().is_some_and(|t| t == ",") => {}
            _ => out.push(token),
        }
    }
    out
}

/// The first place where two token lists differ, with a little context, for the error message.
pub(crate) fn first_difference(before: &[String], after: &[String]) -> Option<String> {
    let index = before
        .iter()
        .zip(after.iter())
        .position(|(a, b)| a != b)
        .or_else(|| (before.len() != after.len()).then(|| before.len().min(after.len())))?;
    let window = |tokens: &[String]| {
        let from = index.saturating_sub(3);
        let to = (index + 4).min(tokens.len());
        tokens[from..to].join(" ")
    };
    Some(format!("source `{}` would become `{}`", window(before), window(after)))
}
