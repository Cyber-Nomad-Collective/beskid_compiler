use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::resolve::{ResolveError, ResolveWarning};

pub(crate) fn emit_resolve_error(ctx: &mut RuleContext, error: ResolveError) {
    match error {
        ResolveError::DuplicateItem { name, span, previous } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveDuplicateItem { name, previous });
        }
        ResolveError::DuplicateSymbol { symbol, span, previous } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveDuplicateItem { name: symbol, previous });
        }
        ResolveError::DuplicateLocal { name, span, previous } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveDuplicateLocal { name, previous });
        }
        ResolveError::UnknownValue { name, span, suggestion } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveUnknownValue { name, suggestion });
        }
        ResolveError::UnknownType { name, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveUnknownType { name });
        }
        // E1108 has one producer: the query gate (`unresolved_call_target`,
        // `unresolved_type_reference`, value-path unit obligations) reports an unknown module path
        // in code, and E1105 reports an unknown `use`. The resolver error only stops resolution.
        ResolveError::UnknownModulePath { .. } => {}
        ResolveError::UnknownValueInModule { module_path, name, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveUnknownValueInModule { module_path, name });
        }
        ResolveError::UnknownTypeInModule { module_path, name, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveUnknownTypeInModule { module_path, name });
        }
        ResolveError::InvalidConformanceTarget { name, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveInvalidConformanceTarget { name });
        }
        ResolveError::PrivateItemInModule { module_path, name, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolvePrivateItemInModule { module_path, name });
        }
        ResolveError::AmbiguousItemInModule { module_path, name, candidates, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveAmbiguousItemInModule { module_path, name, candidates });
        }
        ResolveError::AmbiguousModuleQualifier { name, member, import_path, span } => {
            ctx.emit_issue(span, SemanticIssueKind::ResolveAmbiguousModuleQualifier { name, member, import_path });
        }
    }
}

pub(crate) fn emit_resolve_warning(ctx: &mut RuleContext, warning: &ResolveWarning) {
    match warning {
        ResolveWarning::ShadowedLocal { name, span, previous } => {
            ctx.emit_issue(*span, SemanticIssueKind::ResolveShadowedLocal { name: name.clone(), previous: *previous });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{AnalysisOptions, builtin_rules, run_rules};
    use crate::services::parse_program;

    /// The structural rules never report E1108; the query gate is its only producer.
    #[test]
    fn the_resolver_pass_does_not_report_unknown_module_paths() {
        for source in [
            "use Core.Output.WriteLine; pub i64 Main() { return Core.Text.Length(); }",
            "pub unit Main(Core.Text.Thing value) { return; }",
            "pub unit Main() { let f = Core.Text.Thing; return; }",
        ] {
            let program = parse_program(source).expect("parse");
            let result = run_rules(&program.node, "Main.bd", source, &builtin_rules(), AnalysisOptions::default());
            assert!(
                result.diagnostics.iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1108")),
                "{source}: {:?}",
                result.diagnostics
            );
        }
    }
}
