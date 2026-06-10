//! Compile-time `@{}` hole evaluation for mod `CodeString` bodies.

use crate::services::parse_expression_source;
use crate::syntax::expressions::{
    decode_string_literal_token, materialize_code_segments, parse_plain_code_body, Literal,
};
use crate::syntax::Expression;

use super::generate_output::CodeGenerateOutput;

/// Splice `@{}` holes in a Beskid-tagged code body.
///
/// The body is parsed with the `CodePlainBody` grammar (`CodeHole` + `CodePlainText`).
/// Each hole is a Beskid expression; string literals decode via `StringContent`.
pub fn evaluate_beskid_code_body(body: &str) -> Result<String, String> {
    let segments = parse_plain_code_body(body)
        .map_err(|err| format!("failed to parse code body: {err:?}"))?;
    materialize_code_segments(&segments, eval_code_hole)
}

fn eval_code_hole(source: &str) -> Result<String, String> {
    if source.is_empty() {
        return Err("empty `@{}` hole in code string body".into());
    }
    let parsed = parse_expression_source("@code-hole", source)
        .map_err(|err| format!("failed to parse `@{{}}` hole `{source}`: {err}"))?;
    match &parsed.node {
        Expression::Literal(literal) => match &literal.node.literal.node {
            Literal::String(token) => decode_string_literal_token(token).map_err(|err| {
                format!("invalid string literal in `@{{}}` hole `{source}`: {err:?}")
            }),
            other => Err(format!(
                "unsupported `@{{}}` hole literal (expected string, got {other:?})"
            )),
        },
        _ => Err(format!(
            "unsupported `@{{}}` hole expression `{source}` (AOT mod evaluation not wired)"
        )),
    }
}

/// Build a disk materialization record from one SDK `CodeContribution` payload.
pub fn code_generate_output(
    module_path: &str,
    file_name: &str,
    language: &str,
    body: &str,
) -> Result<CodeGenerateOutput, String> {
    let evaluated_body = if language == "beskid" {
        evaluate_beskid_code_body(body)?
    } else {
        body.to_string()
    };
    Ok(CodeGenerateOutput {
        module_path: module_path.to_string(),
        body: evaluated_body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_string_literal_holes() {
        let body = "header\n@{\"line two\"}\ntrailer";
        let out = evaluate_beskid_code_body(body).expect("eval");
        assert_eq!(out, "header\nline two\ntrailer");
    }

    #[test]
    fn passthrough_without_holes() {
        let body = "pub i64 Demo() { return 1; }";
        let out = evaluate_beskid_code_body(body).expect("eval");
        assert_eq!(out, body);
    }

    #[test]
    fn rejects_unclosed_hole() {
        let err = evaluate_beskid_code_body("before @{1 + 2").unwrap_err();
        assert!(err.contains("failed to parse code body"));
    }
}
