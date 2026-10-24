use crate::analysis::diagnostics::{SemanticDiagnostic, Severity, make_diagnostic};
use crate::parser::Rule;
use crate::parsing::diagnose::{codes, diagnose_parse_failure};
use crate::parsing::error::ParseError;
use crate::projects::ProjectError;
use crate::rule_words::rule_words;
use crate::syntax::SpanInfo;

/// A grammar error as a structured parse diagnostic (`E0101`–`E0199`), for a failed `Program` parse.
///
/// Prefer [`crate::services::parse_program_with_source_name_and_diagnostics`], which also offers
/// fixes; this entry point only describes the error.
pub fn pest_error_diagnostic(source_name: &str, source: &str, err: &pest::error::Error<Rule>) -> SemanticDiagnostic {
    diagnose_parse_failure(source_name, source, Rule::Program, err).diagnostic
}

/// A failure while building the syntax tree from a successful grammar parse.
pub fn parse_error_diagnostic(source_name: &str, source: &str, err: &ParseError) -> SemanticDiagnostic {
    let diagnostic = |span: SpanInfo, message: String, label: &str, help: Option<String>, code: &str| {
        make_diagnostic(source_name, source, span, message, label, help, Some(code.to_string()), Severity::Error)
    };
    match err {
        ParseError::UnexpectedRule { expected, found, span } => {
            let found_words = rule_words(*found).render();
            let message = match expected {
                Some(rule) => format!("expected {}, found {found_words}", rule_words(*rule).render()),
                None => format!("unexpected {found_words}"),
            };
            diagnostic(*span, message, "unexpected here", None, codes::UNEXPECTED_TREE)
        }
        ParseError::MissingPair { expected } => diagnostic(
            SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) },
            format!("expected {}", rule_words(*expected).render()),
            "missing here",
            None,
            codes::UNEXPECTED_TREE,
        ),
        ParseError::ForbiddenImplSelfParameter { span } => diagnostic(
            *span,
            "a method receiver is written `this`, not `self`".to_string(),
            "`self` parameter",
            Some("write `this` as the first parameter, or leave the receiver implicit".to_string()),
            codes::SELF_PARAMETER,
        ),
        ParseError::UnknownStringEscape { span, escape } => diagnostic(
            *span,
            format!("unknown escape `{escape}` in string"),
            "unknown escape",
            Some(
                "the escapes are `\\n`, `\\t`, `\\r`, `\\0`, `\\\"`, `\\\\`, `\\${` and `\\u{...}`; write `\\\\` for a backslash"
                    .to_string(),
            ),
            codes::UNKNOWN_ESCAPE,
        ),
        ParseError::InvalidUnicodeEscape { span, escape, reason } => diagnostic(
            *span,
            format!("invalid escape `{escape}`: {reason}"),
            "invalid escape",
            Some("write one to six hexadecimal digits of a Unicode scalar value, for example `\\u{1F600}`".to_string()),
            codes::INVALID_UNICODE_ESCAPE,
        ),
    }
}

pub fn project_error_diagnostic(source_name: &str, source: &str, error: &ProjectError) -> SemanticDiagnostic {
    if let ProjectError::RootBlockNameMismatch { block, name, block_span, name_span } = error {
        return root_block_mismatch_diagnostic(source_name, source, error, block, name, *block_span, *name_span);
    }
    let (span, message): (crate::syntax::SpanInfo, String) = match error {
        ProjectError::ParseAt { line, message, start, end } => {
            let span = if let (Some(s), Some(e)) = (start, end) {
                if *e > *s {
                    crate::syntax::SpanInfo::from_byte_range_in_source(source, *s, *e)
                } else {
                    crate::syntax::SpanInfo::whole_line_in_source(source, *line)
                }
            } else {
                crate::syntax::SpanInfo::whole_line_in_source(source, *line)
            };
            (span, message.clone())
        }
        _ => {
            let end = if source.is_empty() { 0 } else { 1.min(source.len()) };
            (crate::syntax::SpanInfo { start: 0, end, line_col_start: (1, 1), line_col_end: (1, 1) }, error.to_string())
        }
    };

    let code = error.code().to_string();
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        span,
        message.clone(),
        project_error_label(error),
        None,
        Some(code),
        crate::analysis::Severity::Error,
    )
}

/// Short primary label for a manifest, workspace or lock failure; says what is wrong at the span.
fn project_error_label(error: &ProjectError) -> &'static str {
    match error {
        ProjectError::Parse(_) | ProjectError::ParseAt { .. } => "cannot read this",
        ProjectError::Validation(_) | ProjectError::MetaContractViolation { .. } => "invalid manifest",
        ProjectError::RootBlockNameMismatch { .. } => "block name differs from `name`",
        ProjectError::ReadManifest { .. } | ProjectError::ProjectFileNotFound(_) => "manifest not found",
        ProjectError::TargetNotFound(_) => "no such target",
        ProjectError::DependencyManifestNotFound { .. }
        | ProjectError::DependencyCycle(_)
        | ProjectError::UnresolvedExternalDependencies(_)
        | ProjectError::UnsupportedDependencySourceV1 { .. } => "dependency problem",
        ProjectError::LockfileV1 => "Project.lock needs migration",
        ProjectError::LockfileOutOfDate { .. }
        | ProjectError::LockfileFrozenMode
        | ProjectError::LockfileRequired { .. }
        | ProjectError::LockfileRead { .. }
        | ProjectError::LockfileWrite { .. } => "Project.lock problem",
        ProjectError::MaterializationCreateDir { .. }
        | ProjectError::MaterializationLock { .. }
        | ProjectError::MaterializationReadDir { .. }
        | ProjectError::MaterializationMetadata { .. }
        | ProjectError::MaterializationCopy { .. }
        | ProjectError::MaterializationPrune { .. } => "materialization failed",
    }
}

/// The block name anchors the diagnostic, the `name` value is the related location, and the fix
/// renames the block so the two agree.
fn root_block_mismatch_diagnostic(
    source_name: &str,
    source: &str,
    error: &ProjectError,
    block: &str,
    name: &str,
    block_span: Option<(usize, usize)>,
    name_span: Option<(usize, usize)>,
) -> SemanticDiagnostic {
    use crate::analysis::{Applicability, Fix};
    use crate::syntax::SpanInfo;

    // The block span starts at the block, and its first identifier is the block name.
    let block_name = block_span.and_then(|(start, end)| {
        let region = source.get(start..end.min(source.len()))?;
        let offset = region.find(block)?;
        Some((start + offset, start + offset + block.len()))
    });
    // Point at the quoted value of `name = "..."`, falling back to the whole assignment.
    let name_value = name_span.and_then(|(start, end)| {
        let region = source.get(start..end.min(source.len()))?;
        let open = region.find('"')?;
        let close = region.rfind('"')?;
        if close > open { Some((start + open, start + close + 1)) } else { Some((start, end)) }
    });
    let replacement = if is_block_identifier(name) { name.to_string() } else { name.replace('-', "_") };
    let anchor = block_name.map(|(s, e)| SpanInfo::from_byte_range_in_source(source, s, e)).unwrap_or(SpanInfo {
        start: 0,
        end: if source.is_empty() { 0 } else { 1 },
        line_col_start: (1, 1),
        line_col_end: (1, 1),
    });
    let mut diagnostic = crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        anchor,
        error.to_string(),
        project_error_label(error),
        None,
        Some(error.code().to_string()),
        crate::analysis::Severity::Error,
    );
    if let Some((s, e)) = name_value {
        diagnostic = diagnostic.with_related(SpanInfo::from_byte_range_in_source(source, s, e), "`name` is set here");
    }
    if block_name.is_some() {
        diagnostic = diagnostic.with_fix(Fix::replace(
            format!("Rename the block to `{replacement}`"),
            anchor,
            replacement,
            Applicability::MachineApplicable,
        ));
    }
    diagnostic
}

fn is_block_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod project_error_tests {
    use super::project_error_diagnostic;
    use crate::analysis::Applicability;
    use crate::projects::{ProjectError, parse_manifest};

    fn text(source: &str, span: miette::SourceSpan) -> &str {
        &source[span.offset()..span.offset() + span.len()]
    }

    const MISMATCH: &str = "Runner {\n  name = \"lings\"\n  version = \"0.1.0\"\n}\n\ntarget \"lings\" {\n  kind = Lib\n}\n";

    #[test]
    fn block_name_mismatch_anchors_on_the_block_name_with_the_name_value_related() {
        let error = parse_manifest(MISMATCH).expect_err("mismatched block name");
        let diagnostic = project_error_diagnostic("Runner.bproj", MISMATCH, &error);

        assert_eq!(text(MISMATCH, diagnostic.span), "Runner");
        assert_eq!(diagnostic.code.as_deref(), Some("E1896"));
        assert_ne!(diagnostic.label, "manifest errmsg");
        assert!(!diagnostic.message.contains("root block kind"), "{}", diagnostic.message);
        assert!(diagnostic.message.contains("`Runner`") && diagnostic.message.contains("`lings`"));

        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(text(MISMATCH, diagnostic.related[0].span), "\"lings\"");

        let fix = diagnostic.fixes.first().expect("rename fix");
        assert_eq!(fix.title, "Rename the block to `lings`");
        assert_eq!(fix.applicability, Applicability::MachineApplicable);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].range(), (0, "Runner".len()));
        assert_eq!(fix.edits[0].replacement, "lings");
    }

    #[test]
    fn hyphenated_names_are_renamed_to_their_underscore_projection() {
        let source = "Wrong {\n  name = \"beskid-runtime\"\n  version = \"0.1.0\"\n}\n\ntarget \"x\" {\n  kind = Lib\n}\n";
        let error = parse_manifest(source).expect_err("mismatch");
        let diagnostic = project_error_diagnostic("m.bproj", source, &error);
        assert_eq!(diagnostic.fixes[0].edits[0].replacement, "beskid_runtime");
        assert_eq!(diagnostic.fixes[0].title, "Rename the block to `beskid_runtime`");
    }

    #[test]
    fn manifest_errors_never_use_the_placeholder_label() {
        let errors = [
            ProjectError::Validation("`version` is required in the project root block".into()),
            ProjectError::Parse("bad".into()),
            ProjectError::LockfileV1,
            ProjectError::LockfileFrozenMode,
            ProjectError::TargetNotFound("x".into()),
        ];
        for error in errors {
            let diagnostic = project_error_diagnostic("m.bproj", "x", &error);
            assert_ne!(diagnostic.label, "manifest errmsg", "{error}");
            assert!(!diagnostic.label.is_empty());
        }
    }
}
