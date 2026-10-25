//! One issue kind rendered against its source: message, label, help and code from the kind, plus
//! the fixes and source-tailored help that need the source text (E1214's declaration form, "did
//! you mean" replacements, unused-item removal).

use super::{BindingForm, DiscardedGrowthRepair, ImmutableBinding, SemanticIssueKind};
use crate::analysis::diagnostics::{Applicability, Fix, FixEdit, SemanticDiagnostic, make_diagnostic};
use crate::syntax::SpanInfo;

/// The diagnostic of `kind` at `span` in `source`. Every diagnostics path renders a kind through
/// this one function, so the help and the fixes are the same on the CLI, in JSON and in the editor.
pub fn issue_diagnostic(
    source_name: &str,
    source: &str,
    span: SpanInfo,
    kind: &SemanticIssueKind,
) -> SemanticDiagnostic {
    let diagnostic = make_diagnostic(
        source_name,
        source,
        span,
        kind.message(),
        kind.label(),
        kind.help(),
        Some(kind.code().to_string()),
        kind.severity(),
    );
    match kind {
        SemanticIssueKind::ImmutableAssignment { name, binding } if binding.form != BindingForm::ForIterator => {
            let diagnostic = match mutable_declaration(source, binding) {
                Some(rewritten) => diagnostic.with_help(format!("write `{rewritten}`")),
                None => diagnostic,
            };
            diagnostic.with_fix(Fix::new(
                format!("Make `{name}` mutable"),
                Applicability::MachineApplicable,
                vec![FixEdit::insert(binding.mut_at, "mut ")],
            ))
        }
        SemanticIssueKind::ResolveUnknownValue { name, suggestion: Some(suggestion) }
        | SemanticIssueKind::TypeUnknownStructField { name, suggestion: Some(suggestion), .. } => {
            match name_token(source, span, name) {
                Some(token) => diagnostic.with_fix(Fix::replace(
                    format!("Change `{name}` to `{suggestion}`"),
                    token,
                    suggestion.clone(),
                    Applicability::MaybeIncorrect,
                )),
                None => diagnostic,
            }
        }
        SemanticIssueKind::GenericBoundNotSatisfied { type_name, declaration_at: Some(at), .. }
            if source.get(..*at).is_some() =>
        {
            let line_start = source[..*at].rfind('\n').map_or(0, |index| index + 1);
            let indent: String = source[line_start..*at].chars().take_while(|c| c.is_whitespace()).collect();
            if source[line_start..*at].trim().is_empty() {
                diagnostic.with_fix(Fix::new(
                    format!("Add `[Serialize]` to `{}`", super::display_type(type_name)),
                    Applicability::MachineApplicable,
                    vec![FixEdit::insert(*at, format!("[Serialize]\n{indent}"))],
                ))
            } else {
                diagnostic
            }
        }
        SemanticIssueKind::DiscardedArrayGrowth { repair, .. } => match repair {
            DiscardedGrowthRepair::MutParameter { name, insert_at } => diagnostic.with_fix(Fix::new(
                format!("Declare `{name}` `mut`"),
                Applicability::MachineApplicable,
                vec![FixEdit::insert(*insert_at, "mut ")],
            )),
            DiscardedGrowthRepair::Assign { name, insert_at } => diagnostic.with_fix(Fix::new(
                format!("Assign the result to `{name}`"),
                Applicability::MachineApplicable,
                vec![FixEdit::insert(*insert_at, format!("{name} = "))],
            )),
            DiscardedGrowthRepair::Bind { declaration: Some(declaration), insert_at } => diagnostic.with_fix(Fix::new(
                "Bind the result to a new local",
                Applicability::MaybeIncorrect,
                vec![FixEdit::insert(*insert_at, format!("{declaration} grown = "))],
            )),
            DiscardedGrowthRepair::Bind { declaration: None, .. } => diagnostic,
        },
        SemanticIssueKind::UnusedPrivateItem { name, item, .. } => diagnostic.with_fix(Fix::replace(
            format!("Remove `{name}`"),
            whole_lines(source, *item),
            "",
            Applicability::MaybeIncorrect,
        )),
        _ => diagnostic,
    }
}

/// The declaration with `mut` inserted where the grammar puts it, as the help shows it. A
/// declaration that spans lines or is long is cut after the name.
fn mutable_declaration(source: &str, binding: &ImmutableBinding) -> Option<String> {
    let text = source.get(binding.declaration.start..binding.declaration.end)?;
    let at = binding.mut_at.checked_sub(binding.declaration.start)?;
    let (before, after) = (text.get(..at)?, text.get(at..)?);
    let rewritten = format!("{before}mut {after}");
    if rewritten.contains('\n') || rewritten.len() > 72 {
        // Keep the head up to the first `=`: `mut i32 count`.
        let head = rewritten.split(['=', '\n']).next()?.trim_end();
        return Some(head.to_string());
    }
    Some(rewritten.trim().to_string())
}

/// The bytes of `name` inside `span`: the span's start (`height: 4`, `count`) or its end
/// (`point.heigth`).
fn name_token(source: &str, span: SpanInfo, name: &str) -> Option<SpanInfo> {
    let text = source.get(span.start..span.end)?;
    let start = if text.starts_with(name) {
        span.start
    } else if text.ends_with(name) {
        span.end - name.len()
    } else {
        return None;
    };
    Some(SpanInfo { start, end: start + name.len(), ..span })
}

/// `span` widened to whole lines when only whitespace surrounds it on its first and last line,
/// so removing an item leaves no blank indentation behind.
fn whole_lines(source: &str, span: SpanInfo) -> SpanInfo {
    let bytes = source.as_bytes();
    let end = span.end.min(source.len());
    let mut start = span.start.min(end);
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    if source[line_start..start].trim().is_empty() {
        start = line_start;
    }
    let line_end = source[end..].find('\n').map_or(source.len(), |index| end + index);
    let end = if source[end..line_end].trim().is_empty() {
        if line_end < bytes.len() { line_end + 1 } else { line_end }
    } else {
        end
    };
    SpanInfo { start, end, ..span }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Severity;

    fn span(source: &str, needle: &str) -> SpanInfo {
        let start = source.find(needle).expect("needle");
        SpanInfo { start, end: start + needle.len(), line_col_start: (1, 1), line_col_end: (1, 1) }
    }

    fn apply(source: &str, diagnostic: &SemanticDiagnostic) -> String {
        let mut text = source.to_string();
        let mut edits = diagnostic.fixes[0].edits.clone();
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.range().0));
        for edit in edits {
            let (start, end) = edit.range();
            text.replace_range(start..end, &edit.replacement);
        }
        text
    }

    #[test]
    fn immutable_typed_local_gets_its_own_form_and_a_preferred_fix() {
        let source = "unit Main() {\n    i32 count = 0;\n    count = count + 1;\n}\n";
        let statement = span(source, "i32 count = 0;");
        let binding =
            ImmutableBinding { form: BindingForm::TypedLocal, declaration: statement, mut_at: statement.start };
        let kind = SemanticIssueKind::ImmutableAssignment { name: "count".into(), binding };
        let diagnostic = issue_diagnostic("Main.bd", source, span(source, "count"), &kind)
            .with_related(span(source, "count = count"), "assigned again");
        assert_eq!(diagnostic.message, "`count` is assigned again but is not mutable");
        assert_eq!(diagnostic.code.as_deref(), Some("E1214"));
        assert_eq!(diagnostic.help.as_deref(), Some("write `mut i32 count = 0;`"));
        assert!(!diagnostic.help.as_deref().unwrap_or_default().contains("let mut"));
        assert!(diagnostic.fixes[0].is_preferred());
        assert_eq!(diagnostic.related[0].label, "assigned again");
        assert!(apply(source, &diagnostic).contains("    mut i32 count = 0;\n"));
    }

    #[test]
    fn immutable_inferred_local_and_parameter_forms() {
        let source = "unit F(i32 limit) { let total = 0; total = 1; limit = 2; }";
        let let_span = span(source, "let total = 0;");
        let inferred = SemanticIssueKind::ImmutableAssignment {
            name: "total".into(),
            binding: ImmutableBinding {
                form: BindingForm::InferredLocal,
                declaration: let_span,
                mut_at: let_span.start + 4,
            },
        };
        let diagnostic = issue_diagnostic("F.bd", source, span(source, "total"), &inferred);
        assert_eq!(diagnostic.help.as_deref(), Some("write `let mut total = 0;`"));
        assert!(apply(source, &diagnostic).contains("let mut total = 0;"));

        let parameter_span = span(source, "i32 limit");
        let parameter = SemanticIssueKind::ImmutableAssignment {
            name: "limit".into(),
            binding: ImmutableBinding {
                form: BindingForm::Parameter,
                declaration: parameter_span,
                mut_at: parameter_span.start,
            },
        };
        let diagnostic = issue_diagnostic("F.bd", source, span(source, "limit"), &parameter);
        assert_eq!(diagnostic.help.as_deref(), Some("write `mut i32 limit`"));
        assert!(apply(source, &diagnostic).starts_with("unit F(mut i32 limit)"));
    }

    #[test]
    fn for_iterators_get_no_mut_fix() {
        let source = "unit F() { for i in range(0, 3) { i = 1; } }";
        let kind = SemanticIssueKind::ImmutableAssignment {
            name: "i".into(),
            binding: ImmutableBinding::of_for_iterator(span(source, "i in")),
        };
        let diagnostic = issue_diagnostic("F.bd", source, span(source, "i in"), &kind);
        assert!(diagnostic.fixes.is_empty());
        assert!(diagnostic.help.as_deref().is_some_and(|help| help.contains("`for` iterator")));
    }

    #[test]
    fn did_you_mean_replaces_only_the_name_and_is_not_preferred() {
        let source = "i32 Main(Point point) { i32 counter = 0; return countr + point.heigth; }";
        let local =
            SemanticIssueKind::ResolveUnknownValue { name: "countr".into(), suggestion: Some("counter".into()) };
        let diagnostic = issue_diagnostic("Main.bd", source, span(source, "countr"), &local);
        assert_eq!(diagnostic.help.as_deref(), Some("a name with a similar spelling exists: `counter`"));
        assert!(!diagnostic.fixes[0].is_preferred());
        assert_eq!(diagnostic.fixes[0].editor_title(), "Change `countr` to `counter` (check the result)");
        assert!(apply(source, &diagnostic).contains("return counter + point.heigth"));

        let field = SemanticIssueKind::TypeUnknownStructField {
            name: "heigth".into(),
            type_name: Some("App::Point".into()),
            fields: vec!["width".into(), "height".into()],
            suggestion: Some("height".into()),
        };
        let diagnostic = issue_diagnostic("Main.bd", source, span(source, "point.heigth"), &field);
        assert_eq!(diagnostic.message, "type `Point` has no field `heigth`");
        assert!(apply(source, &diagnostic).contains("point.height;"));
    }

    #[test]
    fn unknown_field_without_a_close_name_lists_the_fields() {
        let kind = SemanticIssueKind::TypeUnknownStructField {
            name: "depth".into(),
            type_name: Some("Rectangle".into()),
            fields: vec!["width".into()],
            suggestion: None,
        };
        assert_eq!(kind.message(), "type `Rectangle` has no field `depth`");
        assert_eq!(kind.help().as_deref(), Some("its fields are: `width`"));
        assert!(!kind.message().contains("struct") && !kind.label().contains("struct"));
    }

    #[test]
    fn unused_items_are_w2101_unnecessary_with_a_removal_fix() {
        let source = "unit Main() { }\n\nunit Helper() {\n    return;\n}\n";
        let item = span(source, "unit Helper() {\n    return;\n}");
        let kind = SemanticIssueKind::UnusedPrivateItem { name: "Helper".into(), kind: "function".into(), item };
        let diagnostic = issue_diagnostic("Main.bd", source, span(source, "Helper"), &kind);
        assert_eq!(diagnostic.code.as_deref(), Some("W2101"));
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(diagnostic.message, "`Helper` is never used");
        assert_eq!(diagnostic.tags, [crate::analysis::DiagnosticTag::Unnecessary]);
        assert!(!diagnostic.fixes[0].is_preferred());
        assert_eq!(apply(source, &diagnostic), "unit Main() { }\n\n");
    }

    #[test]
    fn a_missing_serializable_conformance_asks_for_serialize() {
        let kind = SemanticIssueKind::GenericBoundNotSatisfied {
            type_name: "App::Point".into(),
            contract_name: "Core::Serialization::Serializable<Json>".into(),
            reason: None,
            declaration_at: None,
        };
        assert_eq!(
            kind.help().as_deref(),
            Some("add `[Serialize]` to type `Point` so the compiler derives its `Serializable` conformance")
        );
    }

    #[test]
    fn a_missing_serialize_attribute_gets_a_preferred_insert_fix() {
        let source = "mod App;\n\n    pub type Point { i64 x; }\n\nunit Main() { Send(Point { x: 1 }); }\n";
        let at = source.find("pub type Point").unwrap();
        let kind = SemanticIssueKind::GenericBoundNotSatisfied {
            type_name: "App::Point".into(),
            contract_name: "Serializable<Json>".into(),
            reason: None,
            declaration_at: Some(at),
        };
        let diagnostic = issue_diagnostic("Main.bd", source, span(source, "Point { x: 1 }"), &kind);
        assert!(diagnostic.fixes[0].is_preferred());
        assert_eq!(diagnostic.fixes[0].title, "Add `[Serialize]` to `Point`");
        assert!(apply(source, &diagnostic).contains("\n    [Serialize]\n    pub type Point { i64 x; }"));
    }

    #[test]
    fn capture_and_function_type_wording_uses_beskid_terms() {
        let kind = SemanticIssueKind::CaptureNotTransferable {
            name: "order".into(),
            type_name: "Main::Order".into(),
            class: "Never".into(),
        };
        let message = kind.message();
        assert_eq!(
            message,
            "`order` cannot be captured here because a `Order` value never leaves the task that created it"
        );
        assert!(!message.contains("::") && !message.contains("transfer class"));
        let mismatch =
            SemanticIssueKind::TypeMismatch { expected_name: "() -> i32".into(), actual_name: "(i64) -> i32".into() };
        assert_eq!(mismatch.message(), "type mismatch: expected `() => i32`, got `(i64) => i32`");
    }

    #[test]
    fn module_paths_render_with_dots() {
        let kind = SemanticIssueKind::ResolveUnknownModulePath { path: "Core::Text".into() };
        assert_eq!(kind.message(), "unknown module path `Core.Text`");
        assert!(!kind.help().unwrap_or_default().contains("::"));
        let value =
            SemanticIssueKind::ResolveUnknownValueInModule { module_path: "Core::Text".into(), name: "Trim".into() };
        assert_eq!(value.message(), "unknown value `Trim` in module `Core.Text`");
    }

    #[test]
    fn import_and_module_declaration_help_wording() {
        let item = SemanticIssueKind::UnknownImportPath {
            path: "Core.Results.Result".into(),
            module: Some("Core.Results".into()),
        };
        assert_eq!(
            item.help().as_deref(),
            Some("`use` imports modules; write `use Core.Results;` and refer to `Core.Results.Result`")
        );
        let duplicate = SemanticIssueKind::DuplicateFileScopedModule { module_path: "App".into() };
        assert_eq!(
            duplicate.help().as_deref(),
            Some(
                "a file declares its own module once with `mod X;` at the top; other files of the module do not repeat it"
            )
        );
    }

    #[test]
    fn result_mismatch_uses_beskid_paths_and_suggests_try_or_match() {
        let kind = SemanticIssueKind::TypeMismatch {
            expected_name: "i32".into(),
            actual_name: "Core::Results::Result<i32, string>".into(),
        };
        assert_eq!(kind.message(), "type mismatch: expected `i32`, got `Result<i32, string>`");
        let help = kind.help().unwrap_or_default();
        assert_eq!(help, "add `?` to propagate the error, or `match` on the result");
        assert!(!help.contains("cast"));
        let option = SemanticIssueKind::TypeMismatch {
            expected_name: "string".into(),
            actual_name: "Core::Options::Option<string>".into(),
        };
        assert_eq!(option.help().as_deref(), Some("add `?` to propagate `None`, or `match` on the option"));
        let unrelated = SemanticIssueKind::TypeMismatch { expected_name: "string".into(), actual_name: "bool".into() };
        assert!(!unrelated.help().unwrap_or_default().contains("cast"));
        let numeric = SemanticIssueKind::TypeMismatch { expected_name: "i64".into(), actual_name: "i32".into() };
        assert_eq!(numeric.help().as_deref(), Some("convert the value with `i64(...)`"));
    }
}
