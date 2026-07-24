use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::hir::{HirBlock, HirExpressionNode, HirProgram};
use crate::resolve::Resolution;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::try_desugar::{TryDesugarTarget, collect_array_for_spans, try_desugar_targets_for_program};

use super::normalizable::Normalize;
use super::{builders, builders::desugar_try_expression};

/// Normalization failures (currently empty; reserved for future HIR transforms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirNormalizeError {
    // Placeholder for future normalization errors
}

impl fmt::Display for HirNormalizeError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

/// In-place desugaring and shape fixes on HIR (for example `try` expansion).
pub fn normalize_program(program: &mut Spanned<HirProgram>) -> Result<(), Vec<HirNormalizeError>> {
    normalize_program_with_resolution(program, None, &[])
}

/// Like [`normalize_program`], using pass-1 [`Resolution`] to desugar `?` from the scrutinee enum.
pub fn normalize_program_with_resolution(
    program: &mut Spanned<HirProgram>,
    resolution: Option<&Resolution>,
    dependency_programs: &[&Spanned<HirProgram>],
) -> Result<(), Vec<HirNormalizeError>> {
    let try_targets =
        resolution.map(|resolution| try_desugar_targets_for_program(resolution, program, dependency_programs));
    let array_for_spans = match resolution {
        Some(resolution) => collect_array_for_spans(resolution, program, dependency_programs),
        None => HashSet::new(),
    };
    let mut normalizer = Normalizer::new(try_targets, array_for_spans);
    normalizer.visit_program(program);
    if normalizer.errors.is_empty() { Ok(()) } else { Err(normalizer.errors) }
}

/// Visitor that applies normalization passes and accumulates [`HirNormalizeError`].
pub struct Normalizer {
    pub(crate) errors: Vec<HirNormalizeError>,
    try_targets: HashMap<SpanInfo, TryDesugarTarget>,
    pub(crate) array_for_spans: HashSet<SpanInfo>,
}

impl Normalizer {
    fn new(try_targets: Option<HashMap<SpanInfo, TryDesugarTarget>>, array_for_spans: HashSet<SpanInfo>) -> Self {
        Self { errors: Vec::new(), try_targets: try_targets.unwrap_or_default(), array_for_spans }
    }

    pub(crate) fn is_array_for_span(&self, span: SpanInfo) -> bool {
        self.array_for_spans.contains(&span)
    }

    fn visit_program(&mut self, program: &mut Spanned<HirProgram>) {
        for item in &mut program.node.items {
            self.visit_item(item);
        }
    }

    fn visit_item(&mut self, item: &mut Spanned<crate::hir::HirItem>) {
        match &mut item.node {
            crate::hir::Item::FunctionDefinition(def) => {
                self.visit_block(&mut def.node.body);
            }
            crate::hir::Item::MethodDefinition(def) => {
                self.visit_block(&mut def.node.body);
            }
            crate::hir::Item::InlineModule(inline) => {
                for nested in &mut inline.node.items {
                    self.visit_item(nested);
                }
            }
            _ => {}
        }
    }

    pub fn visit_block(&mut self, block: &mut Spanned<HirBlock>) {
        let mut new_statements = Vec::new();
        let statements = std::mem::take(&mut block.node.statements);
        for statement in statements {
            let mut expansion = statement.normalize(self);
            new_statements.append(&mut expansion);
        }
        block.node.statements = new_statements;
    }

    pub fn visit_if_statement(&mut self, if_stmt: &mut Spanned<crate::hir::HirIfStatement>) {
        self.visit_expression(&mut if_stmt.node.condition);
        self.visit_block(&mut if_stmt.node.then_block);
        if let Some(else_branch) = &mut if_stmt.node.else_branch {
            match &mut else_branch.node {
                crate::hir::HirElseBranch::Block(block) => self.visit_block(block),
                crate::hir::HirElseBranch::If(nested) => self.visit_if_statement(nested),
            }
        }
    }

    pub fn visit_expression(&mut self, expr: &mut Spanned<HirExpressionNode>) {
        // Pipeline contract: syntax/HIR lowering may contain `TryExpression`; normalization
        // must desugar it so type/codegen backends only observe explicit control-flow.
        if matches!(expr.node, HirExpressionNode::TryExpression(_)) {
            let original =
                std::mem::replace(expr, builders::hir_path_expr("__try_operand_placeholder_desugar", expr.span));
            let HirExpressionNode::TryExpression(mut try_expr) = original.node else {
                unreachable!("guarded by TryExpression match");
            };
            self.visit_expression(&mut try_expr.node.expr);
            let target = self.try_targets.get(&original.span);
            *expr = desugar_try_expression(try_expr, original.span, target);
            self.visit_expression(expr);
            return;
        }

        match &mut expr.node {
            HirExpressionNode::MatchExpression(match_expr) => {
                self.visit_expression(&mut match_expr.node.scrutinee);
                for arm in &mut match_expr.node.arms {
                    if let Some(guard) = &mut arm.node.guard {
                        self.visit_expression(guard);
                    }
                    self.visit_expression(&mut arm.node.value);
                }
            }
            HirExpressionNode::LambdaExpression(lambda_expr) => {
                self.visit_expression(&mut lambda_expr.node.body);
            }
            HirExpressionNode::AssignExpression(assign_expr) => {
                self.visit_expression(&mut assign_expr.node.target);
                self.visit_expression(&mut assign_expr.node.value);
            }
            HirExpressionNode::BinaryExpression(binary_expr) => {
                self.visit_expression(&mut binary_expr.node.left);
                self.visit_expression(&mut binary_expr.node.right);
            }
            HirExpressionNode::UnaryExpression(unary_expr) => {
                self.visit_expression(&mut unary_expr.node.expr);
            }
            HirExpressionNode::CallExpression(call_expr) => {
                self.visit_expression(&mut call_expr.node.callee);
                for arg in &mut call_expr.node.args {
                    self.visit_expression(arg);
                }
            }
            HirExpressionNode::MemberExpression(member_expr) => {
                self.visit_expression(&mut member_expr.node.target);
            }
            HirExpressionNode::StructLiteralExpression(struct_literal) => {
                for field in &mut struct_literal.node.fields {
                    self.visit_expression(&mut field.node.value);
                }
            }
            HirExpressionNode::EnumConstructorExpression(enum_constructor) => {
                for arg in &mut enum_constructor.node.args {
                    self.visit_expression(arg);
                }
            }
            HirExpressionNode::BlockExpression(block_expr) => {
                self.visit_block(&mut block_expr.node.block);
            }
            HirExpressionNode::GroupedExpression(grouped_expr) => {
                self.visit_expression(&mut grouped_expr.node.expr);
            }
            HirExpressionNode::LiteralExpression(_) | HirExpressionNode::PathExpression(_) => {}
            HirExpressionNode::TryExpression(_) => unreachable!("try desugared before traversal"),
            HirExpressionNode::SpawnExpression(spawn_expr) => {
                self.visit_expression(&mut spawn_expr.node.callee);
            }
            HirExpressionNode::IndexExpression(index_expr) => {
                self.visit_expression(&mut index_expr.node.target);
                self.visit_expression(&mut index_expr.node.index);
            }
            HirExpressionNode::ArrayLiteralExpression(lit) => {
                for element in &mut lit.node.elements {
                    self.visit_expression(element);
                }
            }
            HirExpressionNode::MacroInvocation(_) | HirExpressionNode::MacroMetavariable(_) => {}
            HirExpressionNode::CodeStringExpression(_) => {}
        }
    }
}
