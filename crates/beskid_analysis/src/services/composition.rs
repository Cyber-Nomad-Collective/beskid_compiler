use crate::analysis::SemanticDiagnostic;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::make_diagnostic;
use crate::composition::baseline::lib_project_rejects_launch;
use crate::composition::{CompositionInput, CompositionIssue, CompositionResult, CompositionTypeResolver};
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

/// Resolve the entry composition with the other units of its assembly available as base hosts
/// (a library's or Corelib's `host`, such as `ConsoleHost`). `types` is the query layer's type
/// authority (`beskid_queries::CompositionTypes`).
pub fn resolve_program_composition_with_imports(
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    types: &dyn CompositionTypeResolver,
    compile_plan: Option<&CompilePlan>,
) -> CompositionResult {
    crate::composition::resolve_composition_with_imports(
        CompositionInput { program, is_mod_project: is_mod_compile_plan(compile_plan), types },
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

/// The name and text of one imported unit of the assembly, for composition diagnostics located
/// in it (a base host's registry line, an imported service's `inject` field).
#[derive(Debug, Clone, Copy)]
pub struct CompositionUnitSource<'a> {
    pub path: &'a std::path::Path,
    pub name: &'a str,
    pub source: &'a str,
}

/// The units of `assembly` other than `entry`, as diagnostic sources.
pub fn composition_unit_sources<'a>(
    assembly: &'a crate::projects::ProgramAssembly,
    entry: &std::path::Path,
) -> Vec<CompositionUnitSource<'a>> {
    assembly
        .units
        .iter()
        .filter(|unit| unit.path != entry)
        .map(|unit| CompositionUnitSource {
            path: unit.path.as_path(),
            name: unit.logical_name.as_str(),
            source: unit.source.as_str(),
        })
        .collect()
}

/// Diagnostics of a composition result: entry issues in `source_name`, and each issue located in
/// an imported unit in that unit's own file (`units`). A foreign issue whose unit text is not
/// among `units` is reported in the entry at `program_span`, never dropped.
pub fn composition_result_to_diagnostics(
    composition: &CompositionResult,
    program_span: SpanInfo,
    source_name: &str,
    source: &str,
    units: &[CompositionUnitSource<'_>],
    plan: Option<&CompilePlan>,
) -> Vec<SemanticDiagnostic> {
    let mut diagnostics = composition
        .issues
        .iter()
        .map(|issue| composition_issue_to_diagnostic(issue, program_span, source_name, source))
        .collect::<Vec<_>>();
    for (path, issue) in &composition.foreign_issues {
        diagnostics.push(match units.iter().find(|unit| unit.path == path.as_ref()) {
            Some(unit) => composition_issue_to_diagnostic(issue, SpanInfo::default(), unit.name, unit.source),
            None => make_diagnostic(
                source_name,
                source,
                program_span,
                format!("composition issue in `{}` that has no source text: {issue:?}", path.display()),
                "composition issue in another unit",
                None,
                Some(crate::composition::diagnostics::composition_issue_code(issue).to_string()),
                crate::analysis::Severity::Error,
            ),
        });
    }

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
        CompositionIssue::LaunchArgumentCount { host_name, required, total, found, cross_unit: true, span } => (
            *span,
            format!("`launch {host_name}(...)` cannot pass arguments to a host declared in another unit"),
            "host parameters from another unit",
            Some(format!(
                "`{host_name}` declares {} and this `launch` passes {found}; declare the launched host with parameters in the unit that launches it",
                declared_parameters(*required, *total)
            )),
        ),
        CompositionIssue::LaunchArgumentCount { host_name, required, total, found, span, .. } => (
            *span,
            format!(
                "`launch {host_name}(...)` passes {found} argument(s), but `{host_name}` declares {}",
                declared_parameters(*required, *total)
            ),
            "wrong number of launch arguments",
            Some(argument_count_help(&format!("host {host_name}(...)"), *required, *total)),
        ),
        CompositionIssue::WithArgumentCount { scope_name, required, total, found, span } => (
            *span,
            format!(
                "`with {scope_name}(...)` passes {found} argument(s), but scope `{scope_name}` declares {}",
                declared_parameters(*required, *total)
            ),
            "wrong number of scope arguments",
            Some(argument_count_help(&format!("scope {scope_name}(...)"), *required, *total)),
        ),
        CompositionIssue::BaseHostParameters { host_name, span } => (
            *span,
            format!("base host `{host_name}` cannot declare parameters"),
            "parameters on a base host",
            Some(
                "only the launched host receives `launch` arguments, and only its scopes receive `with` arguments; move the parameters to the launched host, or inject a service that holds the value"
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
        CompositionIssue::OverrideLifetimeMismatch {
            binding, lifetime, overridden_span, overridden_lifetime, ..
        } => {
            let diagnostic = match overridden_span {
                Some(span) => diagnostic
                    .with_related(*span, format!("base host registers `{binding}` as `{overridden_lifetime}`")),
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

/// `2 parameter(s)` or `1 required and 2 optional parameter(s)`.
fn declared_parameters(required: usize, total: usize) -> String {
    if required == total {
        format!("{total} parameter(s)")
    } else {
        format!("{required} required and {} optional parameter(s)", total - required)
    }
}

fn argument_count_help(declaration: &str, required: usize, total: usize) -> String {
    if required == total {
        format!("pass one argument for each parameter of `{declaration}`, in order")
    } else {
        format!(
            "pass the {required} required argument(s) of `{declaration}` in order, then up to {} optional one(s); an omitted optional argument takes its default value",
            total - required
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::services::parse_program;

    /// Expansion (with hook lowering) as the build pipeline runs it, then composition with the
    /// test type resolver.
    fn diagnostics(source: &str) -> Vec<crate::analysis::SemanticDiagnostic> {
        let program = parse_program(source).expect("parse");
        let generated = super::prepare_program_for_composition(program, None, "Main.bd", source).expect("expansion");
        let types = crate::composition::types::test_types::DeclaredTypes::new(&generated.program, &[]);
        let result = super::resolve_program_composition_with_imports(&generated.program, &[], &types, None);
        super::composition_result_to_diagnostics(&result, generated.program.span, "Main.bd", source, &[], None)
    }

    #[test]
    fn singular_inject_of_two_registrations_suggests_an_array() {
        let source = r#"
contract Greeter { string Greet(); }
type English : Greeter { pub string Greet() { return "hello"; } }
type Polish : Greeter { pub string Greet() { return "czesc"; } }
type Reception { inject Greeter greeter; }
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
        assert!(fixed.contains("inject Greeter[] greeters;"), "{fixed}");
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
type Report { inject Clock[] clocks; }
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
type Reception { inject Greeter greeter; }
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
    #[test]
    fn launch_and_with_argument_counts_are_checked() {
        let source = r#"
host AppHost(string name)
{
    scope Visit(i32 id) { }
}
i32 Main()
{
    launch AppHost();
    with Visit(1, 2) { }
    return 0;
}
"#;
        let found = diagnostics(source);
        let launch = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1722"))
            .unwrap_or_else(|| panic!("E1722 expected: {found:?}"));
        assert!(launch.message.contains("passes 0 argument(s)"), "{}", launch.message);
        let with = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1708"))
            .unwrap_or_else(|| panic!("E1708 expected: {found:?}"));
        assert!(with.message.contains("declares 1 parameter(s)"), "{}", with.message);

        let matching =
            source.replace("launch AppHost();", "launch AppHost(\"Ada\");").replace("Visit(1, 2)", "Visit(1)");
        let found = diagnostics(&matching);
        assert!(
            found.iter().all(|diagnostic| !matches!(diagnostic.code.as_deref(), Some("E1722" | "E1708"))),
            "{found:?}"
        );
    }

    #[test]
    fn a_base_host_with_parameters_is_rejected() {
        let source = r#"
host BaseHost(string name) { scope Visit(i32 id) { } }
host AppHost() : BaseHost { }
i32 Main() { launch AppHost(); return 0; }
"#;
        let found = diagnostics(source);
        let base = found
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("E1723"))
            .unwrap_or_else(|| panic!("E1723 expected: {found:?}"));
        assert!(base.message.contains("`BaseHost`"), "{}", base.message);
    }
    #[test]
    fn an_imported_registration_is_checked_and_reported_in_its_own_unit() {
        let library_source = r#"
pub type Prefixed { string prefix; }
host BaseHost() { registry { single Prefixed; } }
"#;
        let library = parse_program(library_source).expect("library parses");
        let source = r#"
host AppHost() : BaseHost { }
i32 Main() { launch AppHost(); return 0; }
"#;
        let program = crate::composition::lower_host_hooks(parse_program(source).expect("parse"));
        let path: std::sync::Arc<std::path::Path> = std::sync::Arc::from(std::path::Path::new("/lib/Base.bd"));
        let imported = [(std::sync::Arc::clone(&path), &library)];
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &imported);
        let result = super::resolve_program_composition_with_imports(&program, &imported, &types, None);
        assert!(result.issues.is_empty(), "nothing is wrong in the entry: {:?}", result.issues);
        let [(unit, crate::composition::CompositionIssue::UnconstructibleRegistration { .. })] =
            result.foreign_issues.as_slice()
        else {
            panic!("E1719 in the library unit expected: {:?}", result.foreign_issues)
        };
        assert_eq!(unit, &path);
        let units = [super::CompositionUnitSource { path: &path, name: "Base.bd", source: library_source }];
        let diagnostics =
            super::composition_result_to_diagnostics(&result, program.span, "Main.bd", source, &units, None);
        let [diagnostic] = diagnostics.as_slice() else { panic!("one diagnostic: {diagnostics:?}") };
        assert_eq!(diagnostic.code.as_deref(), Some("E1719"));
        assert_eq!(diagnostic.src.name(), "Base.bd", "reported in the library file");
        let start = diagnostic.span.offset();
        assert_eq!(&library_source[start..start + diagnostic.span.len()], "Prefixed");
    }
}
