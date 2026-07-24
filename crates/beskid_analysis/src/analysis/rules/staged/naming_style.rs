use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::naming_case::{is_keyword_escape, matches_profile};
use crate::naming_program::{NamingRole, walk_program};
use crate::syntax::{Identifier, Program, Spanned};

impl SemanticPipelineRule {
    pub(super) fn stage_naming_style(&self, ctx: &mut RuleContext, program: &Program) {
        walk_program(program, |role, ident| {
            check_naming_role(ctx, role, ident);
        });
    }
}

fn check_naming_role(ctx: &mut RuleContext, role: NamingRole, ident: &Spanned<Identifier>) {
    if should_skip_ident(&ident.node.name) {
        return;
    }
    let profile = role.profile();
    if matches_profile(&ident.node.name, profile) {
        return;
    }
    ctx.emit_issue(ident.span, role.issue(&ident.node.name));
}

fn should_skip_ident(name: &str) -> bool {
    name == "self" || is_keyword_escape(name)
}

trait NamingIssue {
    fn issue(self, name: &str) -> SemanticIssueKind;
}

impl NamingIssue for NamingRole {
    fn issue(self, name: &str) -> SemanticIssueKind {
        let name = name.to_string();
        match self {
            Self::TypeDeclaration => SemanticIssueKind::NamingNotPascalCaseType { name },
            Self::EnumVariant => SemanticIssueKind::NamingNotPascalCaseVariant { name },
            Self::Field => SemanticIssueKind::NamingNotCamelCaseField { name },
            Self::Callable => SemanticIssueKind::NamingNotPascalCaseCallable { name },
            Self::ModuleSegment => SemanticIssueKind::NamingNotPascalCaseModuleSegment { segment: name },
            Self::GenericParameter => SemanticIssueKind::NamingNotPascalCaseGeneric { name },
            Self::Binding => SemanticIssueKind::NamingNotCamelCaseBinding { name },
            Self::Test => SemanticIssueKind::NamingNotSnakeCaseTest { name },
            Self::Macro => SemanticIssueKind::NamingNotCamelCaseMacro { name },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalysisOptions;
    use crate::services::parse_program;

    fn warning_codes(src: &str) -> Vec<String> {
        let program = parse_program(src).expect("parse").node;
        let mut ctx = RuleContext::new("test.bd", src, AnalysisOptions::default());
        walk_program(&program, |role, ident| check_naming_role(&mut ctx, role, ident));
        ctx.diagnostics.into_iter().filter_map(|d| d.code).collect()
    }

    #[test]
    fn bad_type_name_emits_w1630() {
        let codes = warning_codes("pub type bad_name { i32 x }");
        assert!(codes.iter().any(|c| c == "W1630"));
    }

    #[test]
    fn bad_local_binding_emits_w1636() {
        let codes = warning_codes("pub unit f() { let BadLocal = 1; return; }");
        assert!(codes.iter().any(|c| c == "W1636"));
    }

    #[test]
    fn bad_test_name_emits_w1637() {
        let codes = warning_codes("test BadTestName { return; }");
        assert!(codes.iter().any(|c| c == "W1637"));
    }

    #[test]
    fn conforming_names_emit_nothing() {
        let src = r#"
pub type Hub { bool isTty }
pub unit Register(i64 index) { let caps = 0; return; }
test hub_register_accepts_channel { return; }
"#;
        assert!(warning_codes(src).is_empty());
    }
}
