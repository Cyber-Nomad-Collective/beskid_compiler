use crate::analysis::SemanticDiagnostic;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::make_diagnostic;
use crate::composition::baseline::lib_project_rejects_launch;
use crate::composition::{CompositionInput, CompositionIssue, CompositionResult};
use crate::mod_host::{ModHostGenerateResult, ModHostInput, run_through_generate};
use crate::projects::CompilePlan;
use crate::syntax::{Program, SpanInfo, Spanned};

pub fn is_mod_compile_plan(plan: Option<&CompilePlan>) -> bool {
    plan.is_some_and(|compile_plan| compile_plan.target.name == "__mod__")
}

/// Run mod-host generate (including macro expansion) for composition analysis parity with the build spine.
pub fn prepare_program_for_composition(
    program: Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
    source_name: &str,
    source: &str,
) -> anyhow::Result<ModHostGenerateResult> {
    run_through_generate(
        program,
        &ModHostInput {
            semantic_scope: None,
            semantic_authority: None,
            compile_plan,
            source_name,
            source,
            pipeline: None,
            mod_host: &crate::mod_host::ModHost::NoMods,
            cached_target_fingerprint: None,
            syntax_generation_id: None,
        },
    )
}

pub fn resolve_program_composition(
    program: &Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
) -> CompositionResult {
    resolve_program_composition_with_imports(program, &[], compile_plan)
}

/// Resolve the entry composition with the other units of its assembly available as base hosts
/// (a library's or Corelib's `host`, such as `ConsoleHost`).
pub fn resolve_program_composition_with_imports(
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    compile_plan: Option<&CompilePlan>,
) -> CompositionResult {
    crate::composition::resolve_composition_with_imports(
        CompositionInput { program, is_mod_project: is_mod_compile_plan(compile_plan) },
        imported,
    )
}

/// The units of `assembly` other than `entry`, as composition imports.
pub fn composition_imports<'a>(
    assembly: &'a crate::projects::ProgramAssembly,
    entry: &std::path::Path,
) -> Vec<(std::sync::Arc<std::path::Path>, &'a Spanned<Program>)> {
    assembly
        .units
        .iter()
        .filter(|unit| unit.path != entry)
        .map(|unit| (std::sync::Arc::from(unit.path.as_path()), &unit.program))
        .collect()
}

pub fn composition_diagnostics_for_program(
    program: &Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
    source_name: &str,
    source: &str,
) -> anyhow::Result<Vec<SemanticDiagnostic>> {
    composition_diagnostics_for_program_with_imports(program, &[], compile_plan, source_name, source)
}

pub fn composition_diagnostics_for_program_with_imports(
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    compile_plan: Option<&CompilePlan>,
    source_name: &str,
    source: &str,
) -> anyhow::Result<Vec<SemanticDiagnostic>> {
    let generated = prepare_program_for_composition(program.clone(), compile_plan, source_name, source)?;
    let composition_result = resolve_program_composition_with_imports(&generated.program, imported, compile_plan);
    Ok(composition_result_to_diagnostics(
        &composition_result,
        generated.program.span,
        source_name,
        source,
        compile_plan,
    ))
}

pub fn composition_result_to_diagnostics(
    composition: &CompositionResult,
    program_span: SpanInfo,
    source_name: &str,
    source: &str,
    plan: Option<&CompilePlan>,
) -> Vec<SemanticDiagnostic> {
    let mut diagnostics = composition
        .issues
        .iter()
        .map(|issue| composition_issue_to_diagnostic(issue, program_span, source_name, source))
        .collect::<Vec<_>>();

    if plan.is_some_and(|compile_plan| {
        lib_project_rejects_launch(compile_plan.target.kind) && !composition.snapshot.launched_host.is_empty()
    }) {
        let kind = SemanticIssueKind::CompositionLaunchInLibProject;
        diagnostics.push(make_diagnostic(
            source_name,
            source,
            composition.snapshot.launch_span.unwrap_or(program_span),
            kind.message(),
            kind.label(),
            kind.help(),
            Some(kind.code().to_string()),
            kind.severity(),
        ));
    }

    diagnostics
}

fn composition_issue_to_diagnostic(
    issue: &CompositionIssue,
    program_span: SpanInfo,
    source_name: &str,
    source: &str,
) -> SemanticDiagnostic {
    if let Some((span, mapped_kind)) = crate::composition::diagnostics::to_semantic_issue(issue) {
        let diagnostic = make_diagnostic(
            source_name,
            source,
            span,
            mapped_kind.message(),
            mapped_kind.label(),
            mapped_kind.help(),
            Some(mapped_kind.code().to_string()),
            mapped_kind.severity(),
        );
        return with_composition_context(diagnostic, issue);
    }

    let (span, message, label, help) = match issue {
        CompositionIssue::StartupInNamedScope { span } => (
            *span,
            "`startup` cannot be declared inside a named scope".to_string(),
            "startup inside a scope",
            Some(
                "`startup` runs once per launch at global scope: move it to the host body, or use `init` to run code each time the scope is entered"
                    .to_string(),
            ),
        ),
        CompositionIssue::RegisteredContract { contract, implementation_span, candidates, .. } => (
            *implementation_span,
            format!("`{contract}` is a contract and cannot be created; register an implementation"),
            "contract registered as a service",
            Some(match candidates.as_slice() {
                [] => format!(
                    "declare a type that conforms to `{contract}` (`type Name : {contract} {{ ... }}`) and register it with `single Name for {contract};`"
                ),
                [only] => format!("register the type that conforms to it: `single {only} for {contract};`"),
                many => format!(
                    "register one of the types that conform to it ({}), for example `single {} for {contract};`",
                    many.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", "),
                    many[0]
                ),
            }),
        ),
        CompositionIssue::UnconstructibleRegistration { implementation, reason, span } => {
            let (message, help) = match reason {
                crate::composition::UnconstructibleReason::ValueField(field) => (
                    format!("`{implementation}` cannot be registered because it has the value field `{field}`"),
                    format!(
                        "the container creates a service without a constructor and fills only its `inject` fields; keep the state of `{implementation}` in a service it injects, or remove `{field}`"
                    ),
                ),
                crate::composition::UnconstructibleReason::Generic => (
                    format!("`{implementation}` cannot be registered because it is generic"),
                    "register a non-generic type; the container creates one exact type".to_string(),
                ),
                crate::composition::UnconstructibleReason::Enum => (
                    format!("`{implementation}` cannot be registered because it is an enum"),
                    "register a `type`; the container creates objects, not enum values".to_string(),
                ),
            };
            (*span, message, "cannot be created by the container", Some(help))
        }
        CompositionIssue::NonConformingRegistration { implementation, contract, span } => (
            *span,
            format!("`{implementation}` does not conform to `{contract}`"),
            "registered for a contract it does not conform to",
            Some(format!("declare the conformance (`type {implementation} : {contract}`), or register a type that conforms")),
        ),
        CompositionIssue::WithWithoutLaunch { scope_name, span } => (
            *span,
            format!("`with {scope_name}` must follow `launch` in the same function"),
            "no launched host here",
            Some(
                "a scope is activated in the function that launches the host, after the `launch` statement and in its block or a block inside it"
                    .to_string(),
            ),
        ),
        CompositionIssue::UnknownRegistrationId { registration_id, span } => (
            span.unwrap_or(program_span),
            format!("composition produced unknown registration id `{registration_id}` while building dependency graph"),
            "invalid composition registration id",
            Some("check host/scope overrides and registration dependencies".to_string()),
        ),
        other => (
            program_span,
            format!("unmapped composition issue: {other:?}"),
            "unmapped composition issue",
            Some("report this issue variant to the compiler team".to_string()),
        ),
    };
    let diagnostic = make_diagnostic(
        source_name,
        source,
        span,
        message,
        label,
        help,
        Some(crate::composition::diagnostics::composition_issue_code(issue).to_string()),
        crate::analysis::Severity::Error,
    );
    with_composition_context(diagnostic, issue)
}

/// Related places and quick fixes that need the composition issue's own data.
fn with_composition_context(diagnostic: SemanticDiagnostic, issue: &CompositionIssue) -> SemanticDiagnostic {
    match issue {
        CompositionIssue::AmbiguousInject { requested_type, field_name, name_span, type_span, candidates, .. } => {
            let plural_name = plural_field_name(field_name);
            let fix = crate::analysis::diagnostics::Fix::new(
                format!("Inject all registrations: `inject {requested_type}[] {plural_name};`"),
                // Uses of the field still read one service and must change with it.
                crate::analysis::diagnostics::Applicability::MaybeIncorrect,
                // One edit over `Type name`: the parsed type span may include the gap before the name.
                vec![crate::analysis::diagnostics::FixEdit::new(
                    SpanInfo { end: name_span.end, line_col_end: name_span.line_col_end, ..*type_span },
                    format!("{requested_type}[] {plural_name}"),
                )],
            );
            let diagnostic = candidates
                .iter()
                .fold(diagnostic, |diagnostic, span| diagnostic.with_related(*span, format!("registers `{requested_type}`")))
                .with_help(format!(
                    "a singular `inject` needs exactly one registration; to receive every registration of `{requested_type}`, inject an array: `inject {requested_type}[] {plural_name};`"
                ));
            diagnostic.with_fix(fix)
        }
        CompositionIssue::OverrideLifetimeMismatch { binding, lifetime, overridden_span, overridden_lifetime, .. } => {
            let diagnostic = match overridden_span {
                Some(span) => {
                    diagnostic.with_related(*span, format!("base host registers `{binding}` as `{overridden_lifetime}`"))
                }
                None => diagnostic,
            };
            diagnostic.with_help(
                format!(
                    "an override keeps the lifetime of the registration it replaces: register `{binding}` as `{overridden_lifetime}`, not `{lifetime}`"
                ),
            )
        }
        CompositionIssue::RegisteredContract { contract, target, implementation_span, candidates, .. } => {
            // One conforming type makes the fix exact; several leave the choice to the reader.
            let applicability = if candidates.len() == 1 {
                crate::analysis::diagnostics::Applicability::MachineApplicable
            } else {
                crate::analysis::diagnostics::Applicability::MaybeIncorrect
            };
            candidates.iter().fold(diagnostic, |diagnostic, candidate| {
                // `single Greeter;` keys the contract itself; `single Greeter for Other;` keeps its key.
                let replacement =
                    if target.is_some() { candidate.clone() } else { format!("{candidate} for {contract}") };
                let line = match target {
                    Some(target) => format!("single {candidate} for {target};"),
                    None => format!("single {candidate} for {contract};"),
                };
                diagnostic.with_fix(crate::analysis::diagnostics::Fix::replace(
                    format!("Register `{candidate}`: `{line}`"),
                    *implementation_span,
                    replacement,
                    applicability,
                ))
            })
        }
        _ => diagnostic,
    }
}

fn plural_field_name(name: &str) -> String {
    if name.ends_with('s') { name.to_string() } else { format!("{name}s") }
}

#[cfg(test)]
mod tests {
    use super::composition_diagnostics_for_program;
    use crate::services::parse_program;

    fn diagnostics(source: &str) -> Vec<crate::analysis::SemanticDiagnostic> {
        let program = parse_program(source).expect("parse");
        composition_diagnostics_for_program(&program, None, "Main.bd", source).expect("composition diagnostics")
    }

    #[test]
    fn singular_inject_of_two_registrations_suggests_an_array() {
        let source = r#"
contract Greeter { string Greet(); }
type English : Greeter { pub string Greet() { return "hello"; } }
type Polish : Greeter { pub string Greet() { return "czesc"; } }
type Reception { inject Greeter greeter, }
host AppHost()
{
    registry
    {
        single English for Greeter;
        single Polish for Greeter;
        single Reception;
    }
}
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        let ambiguous = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1705"))
            .unwrap_or_else(|| panic!("E1705 expected: {found:?}"));
        assert_eq!(ambiguous.related.len(), 2, "both registrations are related places");
        let [fix] = ambiguous.fixes.as_slice() else { panic!("one fix expected") };
        assert!(fix.title.contains("inject Greeter[] greeters;"), "{}", fix.title);
        let mut fixed = source.to_string();
        let mut edits = fix.edits.iter().map(|edit| (edit.range(), edit.replacement.clone())).collect::<Vec<_>>();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.0));
        for ((start, end), text) in edits {
            fixed.replace_range(start..end, &text);
        }
        assert!(fixed.contains("inject Greeter[] greeters,"), "{fixed}");
    }

    #[test]
    fn lifetime_override_mismatch_names_the_binding_and_both_lines() {
        let source = r#"
contract Clock { i64 Now(); }
type SystemClock : Clock { pub i64 Now() { return 0; } }
host BaseHost() { registry { single SystemClock for Clock; } }
host AppHost() : BaseHost { registry { transient SystemClock for Clock; } }
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        let mismatch = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1713"))
            .unwrap_or_else(|| panic!("E1713 expected: {found:?}"));
        assert!(mismatch.message.contains("`Clock`"), "{}", mismatch.message);
        assert!(!mismatch.message.contains("Contract("), "{}", mismatch.message);
        assert_eq!(mismatch.related.len(), 1, "the base registration is the related place");
    }

    #[test]
    fn registrations_in_one_host_are_not_overrides() {
        let source = r#"
contract Clock { i64 Now(); }
type SystemClock : Clock { pub i64 Now() { return 0; } }
type FixedClock : Clock { pub i64 Now() { return 1; } }
type Report { inject Clock[] clocks, }
host AppHost() { registry { single SystemClock for Clock; transient FixedClock for Clock; single Report; } }
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        assert!(found.iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1713")), "{found:?}");
        assert!(found.iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1705")), "{found:?}");
    }

    #[test]
    fn startup_inside_a_scope_is_rejected() {
        let source = r#"
host AppHost() { scope Visit() { startup() { } } }
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        assert!(found.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1717")), "{found:?}");
    }

    #[test]
    fn contract_registered_as_itself_offers_the_conforming_type() {
        let source = r#"
contract Greeter { string Greet(); }
type PoliteGreeter : Greeter { pub string Greet() { return "hello"; } }
type Reception { inject Greeter greeter, }
host AppHost() { registry { single Greeter; single Reception; } }
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        let registered = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1718"))
            .unwrap_or_else(|| panic!("E1718 expected: {found:?}"));
        assert!(registered.message.contains("`Greeter` is a contract"), "{}", registered.message);
        let [fix] = registered.fixes.as_slice() else { panic!("one fix expected: {:?}", registered.fixes) };
        assert_eq!(fix.applicability, crate::analysis::diagnostics::Applicability::MachineApplicable);
        let ((start, end), text) = (fix.edits[0].range(), fix.edits[0].replacement.clone());
        let mut fixed = source.to_string();
        fixed.replace_range(start..end, &text);
        assert!(fixed.contains("single PoliteGreeter for Greeter;"), "{fixed}");
        assert!(diagnostics(&fixed).iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1718")));
    }
}
