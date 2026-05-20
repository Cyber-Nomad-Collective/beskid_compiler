//! Macro expansion driver: fixed-point `name!` substitution with diagnostics.

use crate::analysis::SemanticDiagnostic;
use crate::syntax::expressions::{Expression, MacroInvocation};
use crate::syntax::items::{Node, Program};
use crate::syntax::statements::{Block, Statement};
use crate::syntax::Spanned;

use super::diagnostics::{
    collect_residual_macro_diagnostics, diagnostic_from_kind, diagnostic_from_match_error,
};
use super::match_args::{match_arguments, MatchError};
use super::registry::{macro_name_key, MacroRegistry};
use super::substitute::{bindings_from_pairs, block_body_as_expression, substitute_block};
use super::walk::{map_expression, map_statement};

pub const DEFAULT_MAX_MACRO_EXPANSION_DEPTH: u32 = 32;

pub fn expand_program(program: Spanned<Program>, max_depth: u32) -> Spanned<Program> {
    expand_program_with_diagnostics_impl(program, max_depth, "", "").0
}

pub fn expand_once(
    program: Spanned<Program>,
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
) -> (Spanned<Program>, bool, Vec<SemanticDiagnostic>) {
    let mut changed = false;
    let mut diagnostics = Vec::new();
    let items = program
        .node
        .items
        .iter()
        .flat_map(|item| expand_node(item, registry, source_name, source, &mut changed, &mut diagnostics))
        .collect();
    (
        Spanned::new(
            Program {
                items,
                leading_docs: program.node.leading_docs.clone(),
            },
            program.span,
        ),
        changed,
        diagnostics,
    )
}

fn registry_diags(
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
) -> Vec<SemanticDiagnostic> {
    registry
        .registry_issues
        .iter()
        .map(|(span, kind)| diagnostic_from_kind(source_name, source, *span, kind.clone()))
        .collect()
}

fn expand_node(
    item: &Spanned<Node>,
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
    changed: &mut bool,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) -> Vec<Spanned<Node>> {
    match &item.node {
        Node::Function(f) => {
            let mut n = f.clone();
            n.node.body = expand_block(&f.node.body, registry, source_name, source, changed, diagnostics);
            vec![Spanned::new(Node::Function(n), item.span)]
        }
        Node::InlineModule(m) => {
            let mut n = m.clone();
            n.node.items = n
                .node
                .items
                .iter()
                .flat_map(|i| expand_node(i, registry, source_name, source, changed, diagnostics))
                .collect();
            vec![Spanned::new(Node::InlineModule(n), item.span)]
        }
        _ => vec![item.clone()],
    }
}

fn expand_block(
    block: &Spanned<Block>,
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
    changed: &mut bool,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) -> Spanned<Block> {
    let mut statements = Vec::new();
    for stmt in &block.node.statements {
        if let Statement::Expression(es) = &stmt.node
            && let Expression::MacroInvocation(inv) = &es.node.expression.node
                && inv.node.block.is_some() {
                    match try_expand_block_invocation(inv, registry) {
                        Ok(spliced) => {
                            *changed = true;
                            for s in spliced {
                                statements.push(expand_statement_in_block(
                                    s,
                                    registry,
                                    source_name,
                                    source,
                                    changed,
                                    diagnostics,
                                ));
                            }
                            continue;
                        }
                        Err(err) => {
                            diagnostics.push(diagnostic_from_match_error(
                                source_name, source, &err,
                            ));
                        }
                    }
                }
        statements.push(expand_statement_in_block(
            stmt.clone(),
            registry,
            source_name,
            source,
            changed,
            diagnostics,
        ));
    }
    Spanned::new(Block { statements }, block.span)
}

fn expand_statement_in_block(
    stmt: Spanned<Statement>,
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
    changed: &mut bool,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) -> Spanned<Statement> {
    map_statement(stmt, &mut |expr| {
        expand_expression(expr, registry, source_name, source, changed, diagnostics)
    })
}

fn expand_expression(
    expr: Spanned<Expression>,
    registry: &MacroRegistry,
    source_name: &str,
    source: &str,
    changed: &mut bool,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) -> Spanned<Expression> {
    map_expression(expr, &mut |mapped| {
        if let Expression::MacroInvocation(inv) = &mapped.node {
            match expand_invocation(inv, registry) {
                Ok(expanded) => {
                    *changed = true;
                    return expanded;
                }
                Err(err) => diagnostics.push(diagnostic_from_match_error(source_name, source, &err)),
            }
        }
        mapped
    })
}

fn try_expand_block_invocation(
    inv: &Spanned<MacroInvocation>,
    registry: &MacroRegistry,
) -> Result<Vec<Spanned<Statement>>, MatchError> {
    let name = macro_name_key(&inv.node.name);
    let def = registry.get(&name).ok_or_else(|| MatchError::UnknownMacro {
        name: name.clone(),
        span: inv.span,
    })?;
    let bindings = bindings_from_pairs(match_arguments(
        &inv.node.name,
        &def.node.parameters,
        &inv.node.arguments,
        inv.node.block.as_ref(),
    )?);
    let statements = substitute_block(&def.node.body, &bindings).node.statements;
    Ok(flatten_block_splice_statements(statements))
}

fn flatten_block_splice_statements(statements: Vec<Spanned<Statement>>) -> Vec<Spanned<Statement>> {
    if statements.len() == 1
        && let Statement::Expression(es) = &statements[0].node
            && let Expression::Block(b) = &es.node.expression.node {
                return b.node.block.node.statements.clone();
            }
    statements
}

fn expand_invocation(
    inv: &Spanned<MacroInvocation>,
    registry: &MacroRegistry,
) -> Result<Spanned<Expression>, MatchError> {
    let name = macro_name_key(&inv.node.name);
    let def = registry.get(&name).ok_or_else(|| MatchError::UnknownMacro {
        name: name.clone(),
        span: inv.span,
    })?;
    let bindings = bindings_from_pairs(match_arguments(
        &inv.node.name,
        &def.node.parameters,
        &inv.node.arguments,
        inv.node.block.as_ref(),
    )?);
    Ok(block_body_as_expression(&def.node.body, &bindings, inv.span))
}

pub(crate) fn expand_program_with_diagnostics_impl(
    program: Spanned<Program>,
    max_depth: u32,
    source_name: &str,
    source: &str,
) -> (Spanned<Program>, Vec<SemanticDiagnostic>) {
    let mut diagnostics = Vec::new();
    let mut current = program;

    for _ in 0..max_depth {
        let registry = MacroRegistry::from_program(&current.node);
        diagnostics.extend(registry_diags(&registry, source_name, source));
        let (next, changed, round) = expand_once(current, &registry, source_name, source);
        diagnostics.extend(round);
        current = next;
        if !changed {
            diagnostics.extend(collect_residual_macro_diagnostics(source_name, source, &current));
            return (current, diagnostics);
        }
    }

    let registry = MacroRegistry::from_program(&current.node);
    diagnostics.extend(registry_diags(&registry, source_name, source));
    let (_, still_changing, round) = expand_once(current.clone(), &registry, source_name, source);
    diagnostics.extend(round);
    if still_changing {
        diagnostics.push(diagnostic_from_kind(
            source_name,
            source,
            current.span,
            crate::analysis::diagnostic_kinds::SemanticIssueKind::MacroExpansionDepthExceeded {
                max_depth,
            },
        ));
    }
    diagnostics.extend(collect_residual_macro_diagnostics(source_name, source, &current));
    (current, diagnostics)
}
