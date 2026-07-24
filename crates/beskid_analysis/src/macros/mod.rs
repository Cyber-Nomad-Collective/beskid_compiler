//! Language macro expansion (`macro.expand`): typed AST substitution for `name!` invocations.

mod diagnostics;
mod expand;
mod match_args;
mod registry;
mod substitute;
mod walk;

use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MACRO_EXPAND};

use crate::syntax::{Program, Spanned};

pub use expand::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
pub use match_args::{FragmentBinding, MatchError, fragment_kind_keyword};
pub use registry::MacroRegistry;
pub use substitute::Bindings;

#[derive(Debug, Clone)]
pub struct MacroExpansionOutcome {
    pub program: Spanned<Program>,
    pub diagnostics: Vec<crate::analysis::SemanticDiagnostic>,
}

pub fn expand_program_with_diagnostics(
    program: Spanned<Program>,
    max_depth: u32,
    source_name: &str,
    source: &str,
) -> MacroExpansionOutcome {
    let (program, diagnostics) = expand::expand_program_with_diagnostics_impl(program, max_depth, source_name, source);
    MacroExpansionOutcome { program, diagnostics }
}

pub fn run_macro_expand(
    program: Spanned<Program>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Spanned<Program>> {
    Ok(run_macro_expand_with_diagnostics(program, pipeline, "", "")?.program)
}

pub fn run_macro_expand_with_diagnostics(
    program: Spanned<Program>,
    pipeline: Option<&dyn PipelineObserver>,
    source_name: &str,
    source: &str,
) -> Result<MacroExpansionOutcome> {
    observe_phase_result(pipeline, MACRO_EXPAND, || {
        Ok(expand_program_with_diagnostics(program, DEFAULT_MAX_MACRO_EXPANSION_DEPTH, source_name, source))
    })
}

#[cfg(test)]
mod tests {
    use crate::analysis::diagnostic_kinds::SemanticIssueKind;
    use crate::services::parse_program_with_source_name;
    use crate::syntax::expressions::Expression;
    use crate::syntax::items::MacroFragmentKind;

    use super::*;

    #[test]
    fn duplicate_macro_name_emits_e1907() {
        let source = "macro dup (expression x) { $x; }\nmacro dup (expression y) { $y; }\n";
        let registry =
            MacroRegistry::from_program(&parse_program_with_source_name("M.bd", source).expect("parse").node);
        assert!(registry.registry_issues.iter().any(|(_, k)| matches!(
            k,
            SemanticIssueKind::MacroAmbiguousName { name } if name == "dup"
        )));
    }

    #[test]
    fn unknown_macro_produces_e1901() {
        let source = "unit Main() { missing!(1); return; }\n";
        let outcome = expand_program_with_diagnostics(
            parse_program_with_source_name("Main.bd", source).expect("parse"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            "Main.bd",
            source,
        );
        assert!(outcome.diagnostics.iter().any(|d| d.code.as_deref() == Some("E1901")));
    }

    #[test]
    fn expression_macro_expands_to_binary() {
        let source = r#"
macro twice (expression value) { $value + $value; }
unit Main() { let x = twice!(1); return; }
"#;
        let expanded = expand_program(
            parse_program_with_source_name("Main.bd", source).expect("parse"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let expr = expanded
            .node
            .items
            .iter()
            .find_map(|i| match &i.node {
                crate::syntax::items::Node::Function(f) => {
                    if let crate::syntax::Statement::Let(ls) = &f.node.body.node.statements[0].node {
                        Some(ls.node.value.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("let init");
        assert!(matches!(expr.node, Expression::Binary(_)));
    }

    #[test]
    fn fragment_kind_keywords_match_spec() {
        assert_eq!(fragment_kind_keyword(MacroFragmentKind::Block), "block");
    }

    #[test]
    fn match_arguments_binds_expression_parameter() {
        use crate::syntax::items::{MacroFragmentKind, MacroParameter};
        use crate::syntax::{Identifier, Spanned};

        use super::match_args::match_arguments;

        let name = Spanned::new(Identifier { name: "twice".to_string() }, crate::syntax::SpanInfo::default());
        let param = Spanned::new(
            MacroParameter {
                kind: Spanned::new(MacroFragmentKind::Expression, crate::syntax::SpanInfo::default()),
                name: Spanned::new(Identifier { name: "value".to_string() }, crate::syntax::SpanInfo::default()),
            },
            crate::syntax::SpanInfo::default(),
        );
        let arg = parse_program_with_source_name("A.bd", "unit m() { let x = 1; return; }\n")
            .expect("parse")
            .node
            .items
            .iter()
            .find_map(|item| match &item.node {
                crate::syntax::items::Node::Function(f) => {
                    if let crate::syntax::Statement::Let(ls) = &f.node.body.node.statements[0].node {
                        Some(ls.node.value.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("literal expr");
        let bindings = match_arguments(&name, &[param], &[arg], None).expect("match");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, "value");
    }
}
