//! Token-stream self-check: formatting may change layout, never code.
//!
//! The input and the output are reduced to their code tokens (no whitespace, no comments, no
//! documentation; comments and documentation have their own checks) and compared after the
//! rewrites the house style rules allow:
//! - identifier case and underscores (naming normalization: `bad_name` to `BadName`);
//! - parentheses around `if`, `while` and `match` conditions;
//! - trailing commas before `}` or `)`;
//! - empty `()` (a nullary enum constructor loses its call parentheses);
//! - parentheses around a single lambda parameter.
//!
//! Anything else that differs (a string interpolation printed as `+` concatenation, a lost
//! operator, a changed literal) makes the formatter refuse the file.

use crate::parser::Rule;
use crate::format::trivia::{Comment, is_opaque};
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

/// The comparable code tokens of `source`.
pub(crate) fn code_tokens(source: &str, comments: &[Comment], opaque: &[OpaqueToken]) -> Vec<String> {
    let mut skips: Vec<(usize, usize, Option<Rule>)> = comments.iter().map(|c| (c.start, c.end, None)).collect();
    skips.extend(opaque.iter().map(|t| (t.start, t.end, Some(t.rule))));
    skips.sort_by_key(|s| s.0);

    let mut tokens = Vec::new();
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
                Some(Rule::StringLiteral | Rule::CharLiteral | Rule::StringLiteralValue) => {
                    tokens.push(source[start..end].to_string())
                }
                // Code and CLIF bodies are re-emitted with normalized spacing.
                Some(_) => tokens.push(source[start..end].chars().filter(|c| !c.is_whitespace()).collect()),
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
            // Naming normalization may change case and underscores, nothing else.
            tokens.push(source[start..i].chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect());
        } else {
            tokens.push(ch.to_string());
            i += ch.len_utf8();
        }
    }
    normalize(tokens)
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
    // Parentheses around conditions, removed repeatedly (`if ((a) && (b))` and `if (a) && (b)`
    // both reduce to `if a && b`).
    let mut index = 0;
    while index < tokens.len() {
        if matches!(tokens[index].as_str(), "if" | "while" | "match") {
            while tokens.get(index + 1).is_some_and(|t| t == "(") {
                let Some(close) = matching_close(&tokens, index + 1) else { break };
                tokens.remove(close);
                tokens.remove(index + 1);
            }
        }
        index += 1;
    }
    // A single lambda parameter: `( x ) = >` becomes `x = >`.
    let mut index = 0;
    while index + 4 < tokens.len() {
        if tokens[index] == "("
            && tokens[index + 2] == ")"
            && tokens[index + 3] == "="
            && tokens[index + 4] == ">"
            && tokens[index + 1].chars().all(|c| c.is_alphanumeric())
        {
            tokens.remove(index + 2);
            tokens.remove(index);
        }
        index += 1;
    }
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    for token in tokens {
        match token.as_str() {
            // Trailing comma before a closer.
            "}" | ")" if out.last().is_some_and(|t| t == ",") => {
                out.pop();
                if token == ")" && out.last().is_some_and(|t| t == "(") {
                    out.pop();
                } else {
                    out.push(token);
                }
            }
            // Empty call parentheses.
            ")" if out.last().is_some_and(|t| t == "(") => {
                out.pop();
            }
            _ => out.push(token),
        }
    }
    out
}

/// The first place where two token lists differ, with a little context, for the error message.
pub(crate) fn first_difference(before: &[String], after: &[String]) -> Option<String> {
    let index = before.iter().zip(after.iter()).position(|(a, b)| a != b).or_else(|| {
        (before.len() != after.len()).then(|| before.len().min(after.len()))
    })?;
    let window = |tokens: &[String]| {
        let from = index.saturating_sub(3);
        let to = (index + 4).min(tokens.len());
        tokens[from..to].join(" ")
    };
    Some(format!("source `{}` would become `{}`", window(before), window(after)))
}
