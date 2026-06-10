//! Map macro expansion failures to semantic diagnostics (E1901–E1905, E1907–E1908).

use crate::analysis::SemanticDiagnostic;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::{Severity, make_diagnostic};
use crate::syntax::{SpanInfo, Spanned};

use super::match_args::MatchError;

pub fn diagnostic_from_match_error(
    source_name: &str,
    source: &str,
    error: &MatchError,
) -> SemanticDiagnostic {
    let (span, kind) = match error {
        MatchError::UnknownMacro { span, name } => (
            *span,
            SemanticIssueKind::MacroUnknown { name: name.clone() },
        ),
        MatchError::ArityMismatch {
            span,
            name,
            expected,
            actual,
        } => (
            *span,
            SemanticIssueKind::MacroArgumentArityMismatch {
                name: name.clone(),
                expected: *expected,
                actual: *actual,
            },
        ),
        MatchError::KindMismatch {
            span,
            name,
            parameter,
            expected_kind,
        } => (
            *span,
            SemanticIssueKind::MacroArgumentKindMismatch {
                name: name.clone(),
                parameter: parameter.clone(),
                expected_kind: expected_kind.clone(),
            },
        ),
    };
    make_diagnostic(
        source_name,
        source,
        span,
        kind.message(),
        kind.label(),
        None,
        Some(kind.code().to_string()),
        Severity::Error,
    )
}

pub fn diagnostic_from_kind(
    source_name: &str,
    source: &str,
    span: SpanInfo,
    kind: SemanticIssueKind,
) -> SemanticDiagnostic {
    make_diagnostic(
        source_name,
        source,
        span,
        kind.message(),
        kind.label(),
        None,
        Some(kind.code().to_string()),
        Severity::Error,
    )
}

pub fn collect_residual_macro_diagnostics(
    source_name: &str,
    source: &str,
    program: &Spanned<crate::syntax::Program>,
) -> Vec<SemanticDiagnostic> {
    let mut out = Vec::new();
    for item in &program.node.items {
        scan_node_residuals(source_name, source, item, &mut out);
    }
    out
}

fn scan_node_residuals(
    source_name: &str,
    source: &str,
    item: &Spanned<crate::syntax::items::Node>,
    out: &mut Vec<SemanticDiagnostic>,
) {
    use crate::syntax::items::Node;

    match &item.node {
        Node::Function(f) => scan_block_residuals(source_name, source, &f.node.body, out),
        Node::InlineModule(m) => {
            for child in &m.node.items {
                scan_node_residuals(source_name, source, child, out);
            }
        }
        _ => {}
    }
}

fn scan_block_residuals(
    source_name: &str,
    source: &str,
    block: &Spanned<crate::syntax::Block>,
    out: &mut Vec<SemanticDiagnostic>,
) {
    for stmt in &block.node.statements {
        scan_statement_residuals(source_name, source, stmt, out);
    }
}

fn scan_statement_residuals(
    source_name: &str,
    source: &str,
    stmt: &Spanned<crate::syntax::Statement>,
    out: &mut Vec<SemanticDiagnostic>,
) {
    use crate::syntax::Statement;

    match &stmt.node {
        Statement::Expression(es) => {
            scan_expression_residuals(source_name, source, &es.node.expression, out)
        }
        Statement::Let(ls) => scan_expression_residuals(source_name, source, &ls.node.value, out),
        Statement::Return(rs) => {
            if let Some(v) = &rs.node.value {
                scan_expression_residuals(source_name, source, v, out);
            }
        }
        Statement::If(i) => {
            scan_expression_residuals(source_name, source, &i.node.condition, out);
            scan_block_residuals(source_name, source, &i.node.then_block, out);
            if let Some(else_branch) = &i.node.else_branch {
                match &else_branch.node {
                    crate::syntax::ElseBranch::Block(b) => {
                        scan_block_residuals(source_name, source, b, out);
                    }
                    crate::syntax::ElseBranch::If(nested) => {
                        scan_expression_residuals(source_name, source, &nested.node.condition, out);
                        scan_block_residuals(source_name, source, &nested.node.then_block, out);
                        if let Some(nested_else) = &nested.node.else_branch {
                            match &nested_else.node {
                                crate::syntax::ElseBranch::Block(b) => {
                                    scan_block_residuals(source_name, source, b, out);
                                }
                                crate::syntax::ElseBranch::If(_) => {}
                            }
                        }
                    }
                }
            }
        }
        Statement::While(w) => {
            scan_expression_residuals(source_name, source, &w.node.condition, out);
            scan_block_residuals(source_name, source, &w.node.body, out);
        }
        Statement::For(f) => {
            scan_expression_residuals(source_name, source, &f.node.iterable, out);
            scan_block_residuals(source_name, source, &f.node.body, out);
        }
        Statement::With(w) => {
            for arg in &w.node.arguments {
                scan_expression_residuals(source_name, source, arg, out);
            }
            scan_block_residuals(source_name, source, &w.node.body, out);
        }
        Statement::Launch(l) => {
            for arg in &l.node.arguments {
                scan_expression_residuals(source_name, source, arg, out);
            }
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn scan_expression_residuals(
    source_name: &str,
    source: &str,
    expr: &Spanned<crate::syntax::Expression>,
    out: &mut Vec<SemanticDiagnostic>,
) {
    use crate::syntax::expressions::Expression;

    match &expr.node {
        Expression::MacroInvocation(inv) => {
            let name = super::registry::macro_name_key(&inv.node.name);
            out.push(diagnostic_from_kind(
                source_name,
                source,
                inv.span,
                SemanticIssueKind::MacroUnknown { name },
            ));
        }
        Expression::MacroMetavariable(mv) => {
            out.push(diagnostic_from_kind(
                source_name,
                source,
                mv.span,
                SemanticIssueKind::MacroMetavariableOutsideBody {
                    name: mv.node.name.node.name.clone(),
                },
            ));
        }
        Expression::Block(b) => scan_block_residuals(source_name, source, &b.node.block, out),
        Expression::Assign(a) => {
            scan_expression_residuals(source_name, source, a.node.target.as_ref(), out);
            scan_expression_residuals(source_name, source, a.node.value.as_ref(), out);
        }
        Expression::Binary(b) => {
            scan_expression_residuals(source_name, source, b.node.left.as_ref(), out);
            scan_expression_residuals(source_name, source, b.node.right.as_ref(), out);
        }
        Expression::Unary(u) => {
            scan_expression_residuals(source_name, source, u.node.expr.as_ref(), out);
        }
        Expression::Call(c) => {
            scan_expression_residuals(source_name, source, c.node.callee.as_ref(), out);
            for arg in &c.node.args {
                scan_expression_residuals(source_name, source, arg, out);
            }
        }
        Expression::Member(m) => {
            scan_expression_residuals(source_name, source, m.node.target.as_ref(), out);
        }
        Expression::Grouped(g) => {
            scan_expression_residuals(source_name, source, g.node.expr.as_ref(), out);
        }
        Expression::Try(t) => {
            scan_expression_residuals(source_name, source, t.node.expr.as_ref(), out);
        }
        Expression::Spawn(s) => {
            scan_expression_residuals(source_name, source, s.node.callee.as_ref(), out);
        }
        Expression::Match(m) => {
            scan_expression_residuals(source_name, source, m.node.scrutinee.as_ref(), out);
            for arm in &m.node.arms {
                if let Some(guard) = &arm.node.guard {
                    scan_expression_residuals(source_name, source, guard, out);
                }
                scan_expression_residuals(source_name, source, &arm.node.value, out);
            }
        }
        Expression::Lambda(l) => {
            scan_expression_residuals(source_name, source, l.node.body.as_ref(), out);
        }
        Expression::StructLiteral(s) => {
            for field in &s.node.fields {
                scan_expression_residuals(source_name, source, &field.node.value, out);
            }
        }
        Expression::EnumConstructor(e) => {
            for arg in &e.node.args {
                scan_expression_residuals(source_name, source, arg, out);
            }
        }
        Expression::Literal(_) | Expression::Path(_) => {}
        Expression::Index(i) => {
            scan_expression_residuals(source_name, source, i.node.target.as_ref(), out);
            scan_expression_residuals(source_name, source, i.node.index.as_ref(), out);
        }
        Expression::ArrayLiteral(a) => {
            for elem in &a.node.elements {
                scan_expression_residuals(source_name, source, elem, out);
            }
        }
        Expression::CodeString(_) => {}
    }
}
