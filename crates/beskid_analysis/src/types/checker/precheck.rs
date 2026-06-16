//! Pre-normalize type queries using merged unit surfaces (try `?` targets, array for-loops).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::hir::{HirExpressionNode, HirItem, HirProgram, HirStatementNode};
use crate::resolve::Resolution;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::surface::{build_unit_type_surface, merge_unit_surfaces_with_types};
use crate::types::TypeInfo;

use super::TypeChecker;

/// Resolved enum used when lowering `expr?` to `match`.
#[derive(Debug, Clone)]
pub struct TryDesugarTarget {
    pub type_name: String,
    pub ok_variant: String,
}

const PRECHECK_ROOT: &str = "__precheck";

/// Build a [`TypeChecker`] seeded from merged surfaces for the given programs (last = entry).
pub(crate) fn precheck_checker<'a>(
    resolution: &'a Resolution,
    programs: &[&'a Spanned<HirProgram>],
) -> TypeChecker<'a> {
    let Some(entry) = programs.last().copied() else {
        return TypeChecker::new(resolution, &Default::default());
    };

    let root = Path::new(PRECHECK_ROOT);
    let entry_path = root.join("entry");
    let entry_surface = Arc::new(build_unit_type_surface(entry, resolution, &entry_path));

    let dependency_surfaces = programs[..programs.len().saturating_sub(1)]
        .iter()
        .enumerate()
        .map(|(index, program)| {
            let path = root.join(format!("dep_{index}"));
            (
                path.clone(),
                Arc::new(build_unit_type_surface(program, resolution, &path)),
            )
        });

    let (merged_types, merged) =
        merge_unit_surfaces_with_types(dependency_surfaces, entry_surface);
    let mut checker = TypeChecker::from_merged(resolution, &merged, merged_types);
    for program in programs {
        checker.seed_program_enums(program);
    }
    checker
}

impl<'a> TypeChecker<'a> {
    /// Infer `Result`-shaped enum metadata for a `?` operand.
    pub fn try_desugar_target_for_operand(
        &mut self,
        operand: &Spanned<HirExpressionNode>,
    ) -> Option<TryDesugarTarget> {
        let target_type = self.infer_expression_type(operand)?;
        let item_id = self.item_for_type_id(target_type)?;
        let type_name = self
            .resolution
            .items
            .iter()
            .find(|info| info.id == item_id)
            .map(|info| info.name.clone())?;
        let ok_variant = self.variant_display_name(item_id, "Ok")?;
        Some(TryDesugarTarget {
            type_name,
            ok_variant,
        })
    }

    /// True when the iterable expression type is `T[]`.
    pub fn is_array_iterable(&mut self, iterable: &Spanned<HirExpressionNode>) -> bool {
        let Some(target_type) = self.infer_expression_type(iterable) else {
            return false;
        };
        matches!(self.type_table.get(target_type), Some(TypeInfo::Array(_)))
    }

    /// Spans of `?` operands that are not a `Result`-shaped enum.
    pub fn invalid_try_expression_spans(
        resolution: &'a Resolution,
        entry: &Spanned<HirProgram>,
    ) -> Vec<SpanInfo> {
        let programs: Vec<&Spanned<HirProgram>> = vec![entry];
        let mut checker = precheck_checker(resolution, &programs);
        let mut spans = Vec::new();
        collect_invalid_try_targets(&mut checker, entry, &mut spans);
        spans
    }

    /// Map try-expression span → desugar metadata (computed before in-place normalization).
    pub fn try_desugar_targets_for_program(
        resolution: &'a Resolution,
        entry: &Spanned<HirProgram>,
        dependency_programs: &[&Spanned<HirProgram>],
    ) -> HashMap<SpanInfo, TryDesugarTarget> {
        let mut programs: Vec<&Spanned<HirProgram>> = dependency_programs.to_vec();
        programs.push(entry);
        let mut checker = precheck_checker(resolution, &programs);
        let mut map = HashMap::new();
        collect_try_targets(&mut checker, entry, &mut map);
        map
    }

    /// Map for-statement span → true when the iterable type is `T[]`.
    pub fn collect_array_for_spans(
        resolution: &'a Resolution,
        entry: &Spanned<HirProgram>,
        dependency_programs: &[&Spanned<HirProgram>],
    ) -> HashSet<SpanInfo> {
        let mut programs: Vec<&Spanned<HirProgram>> = dependency_programs.to_vec();
        programs.push(entry);
        let mut checker = precheck_checker(resolution, &programs);
        let mut set = HashSet::new();
        collect_array_fors(&mut checker, entry, &mut set);
        set
    }
}

fn collect_invalid_try_targets(
    checker: &mut TypeChecker<'_>,
    program: &Spanned<HirProgram>,
    spans: &mut Vec<SpanInfo>,
) {
    for item in &program.node.items {
        collect_invalid_try_targets_item(checker, item, spans);
    }
}

fn collect_invalid_try_targets_item(
    checker: &mut TypeChecker<'_>,
    item: &Spanned<HirItem>,
    spans: &mut Vec<SpanInfo>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_invalid_try_targets_in_block(checker, &def.node.body, spans);
        }
        HirItem::MethodDefinition(def) => {
            collect_invalid_try_targets_in_block(checker, &def.node.body, spans);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_invalid_try_targets_item(checker, nested, spans);
            }
        }
        _ => {}
    }
}

fn collect_invalid_try_targets_in_block(
    checker: &mut TypeChecker<'_>,
    block: &Spanned<crate::hir::HirBlock>,
    spans: &mut Vec<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ExpressionStatement(expr_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(
                checker,
                &expr_stmt.node.expression,
                spans,
            );
        } else if let HirStatementNode::LetStatement(let_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(checker, &let_stmt.node.value, spans);
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
            collect_invalid_try_targets_in_expression(checker, value, spans);
        }
    }
}

fn collect_invalid_try_targets_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<HirExpressionNode>,
    spans: &mut Vec<SpanInfo>,
) {
    if let HirExpressionNode::TryExpression(try_expr) = &expr.node
        && checker
            .try_desugar_target_for_operand(&try_expr.node.expr)
            .is_none()
    {
        spans.push(expr.span);
    }
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_invalid_try_targets_in_expression(checker, &binary.node.left, spans);
            collect_invalid_try_targets_in_expression(checker, &binary.node.right, spans);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_invalid_try_targets_in_expression(checker, &unary.node.expr, spans);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_invalid_try_targets_in_expression(checker, &call.node.callee, spans);
            for arg in &call.node.args {
                collect_invalid_try_targets_in_expression(checker, arg, spans);
            }
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_invalid_try_targets_in_expression(checker, &match_expr.node.scrutinee, spans);
            for arm in &match_expr.node.arms {
                collect_invalid_try_targets_in_expression(checker, &arm.node.value, spans);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_invalid_try_targets_in_block(checker, &block_expr.node.block, spans);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_invalid_try_targets_in_expression(checker, &grouped.node.expr, spans);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_invalid_try_targets_in_expression(checker, &assign.node.target, spans);
            collect_invalid_try_targets_in_expression(checker, &assign.node.value, spans);
        }
        _ => {}
    }
}

fn collect_try_targets(
    checker: &mut TypeChecker<'_>,
    program: &Spanned<HirProgram>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for item in &program.node.items {
        collect_try_targets_item(checker, item, map);
    }
}

fn collect_try_targets_item(
    checker: &mut TypeChecker<'_>,
    item: &Spanned<HirItem>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_try_targets_in_block(checker, &def.node.body, map);
        }
        HirItem::MethodDefinition(def) => {
            collect_try_targets_in_block(checker, &def.node.body, map);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_try_targets_item(checker, nested, map);
            }
        }
        _ => {}
    }
}

fn collect_try_targets_in_block(
    checker: &mut TypeChecker<'_>,
    block: &Spanned<crate::hir::HirBlock>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ExpressionStatement(expr_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &expr_stmt.node.expression, map);
        } else if let HirStatementNode::LetStatement(let_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &let_stmt.node.value, map);
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
            collect_try_targets_in_expression(checker, value, map);
        }
    }
}

fn collect_try_targets_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<HirExpressionNode>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    if let HirExpressionNode::TryExpression(try_expr) = &expr.node
        && let Some(target) = checker.try_desugar_target_for_operand(&try_expr.node.expr)
    {
        map.insert(expr.span, target);
    }
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_try_targets_in_expression(checker, &binary.node.left, map);
            collect_try_targets_in_expression(checker, &binary.node.right, map);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_try_targets_in_expression(checker, &unary.node.expr, map);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_try_targets_in_expression(checker, &call.node.callee, map);
            for arg in &call.node.args {
                collect_try_targets_in_expression(checker, arg, map);
            }
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_try_targets_in_expression(checker, &match_expr.node.scrutinee, map);
            for arm in &match_expr.node.arms {
                collect_try_targets_in_expression(checker, &arm.node.value, map);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_try_targets_in_block(checker, &block_expr.node.block, map);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_try_targets_in_expression(checker, &grouped.node.expr, map);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_try_targets_in_expression(checker, &assign.node.target, map);
            collect_try_targets_in_expression(checker, &assign.node.value, map);
        }
        _ => {}
    }
}

fn collect_array_fors(
    checker: &mut TypeChecker<'_>,
    program: &Spanned<HirProgram>,
    set: &mut HashSet<SpanInfo>,
) {
    for item in &program.node.items {
        collect_array_fors_item(checker, item, set);
    }
}

fn collect_array_fors_item(
    checker: &mut TypeChecker<'_>,
    item: &Spanned<HirItem>,
    set: &mut HashSet<SpanInfo>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_array_fors_in_block(checker, &def.node.body, set);
        }
        HirItem::MethodDefinition(def) => {
            collect_array_fors_in_block(checker, &def.node.body, set);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_array_fors_item(checker, nested, set);
            }
        }
        _ => {}
    }
}

fn collect_array_fors_in_else_branch(
    checker: &mut TypeChecker<'_>,
    else_branch: &Spanned<crate::hir::HirElseBranch>,
    set: &mut HashSet<SpanInfo>,
) {
    match &else_branch.node {
        crate::hir::HirElseBranch::Block(block) => {
            collect_array_fors_in_block(checker, block, set);
        }
        crate::hir::HirElseBranch::If(nested) => {
            collect_array_fors_in_expression(checker, &nested.node.condition, set);
            collect_array_fors_in_block(checker, &nested.node.then_block, set);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_array_fors_in_else_branch(checker, nested_else, set);
            }
        }
    }
}

fn collect_array_fors_in_block(
    checker: &mut TypeChecker<'_>,
    block: &Spanned<crate::hir::HirBlock>,
    set: &mut HashSet<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ForStatement(for_stmt) = &statement.node
            && checker.is_array_iterable(&for_stmt.node.iterable)
        {
            set.insert(statement.span);
        }
        collect_array_fors_in_statement(checker, statement, set);
    }
}

fn collect_array_fors_in_statement(
    checker: &mut TypeChecker<'_>,
    statement: &Spanned<HirStatementNode>,
    set: &mut HashSet<SpanInfo>,
) {
    match &statement.node {
        HirStatementNode::LetStatement(let_stmt) => {
            collect_array_fors_in_expression(checker, &let_stmt.node.value, set);
        }
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(value) = &ret.node.value {
                collect_array_fors_in_expression(checker, value, set);
            }
        }
        HirStatementNode::WhileStatement(while_stmt) => {
            collect_array_fors_in_expression(checker, &while_stmt.node.condition, set);
            collect_array_fors_in_block(checker, &while_stmt.node.body, set);
        }
        HirStatementNode::IfStatement(if_stmt) => {
            collect_array_fors_in_expression(checker, &if_stmt.node.condition, set);
            collect_array_fors_in_block(checker, &if_stmt.node.then_block, set);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_array_fors_in_else_branch(checker, else_branch, set);
            }
        }
        HirStatementNode::ExpressionStatement(expr_stmt) => {
            collect_array_fors_in_expression(checker, &expr_stmt.node.expression, set);
        }
        HirStatementNode::ForStatement(_) => {}
        _ => {}
    }
}

fn collect_array_fors_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<HirExpressionNode>,
    set: &mut HashSet<SpanInfo>,
) {
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_array_fors_in_expression(checker, &binary.node.left, set);
            collect_array_fors_in_expression(checker, &binary.node.right, set);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_array_fors_in_expression(checker, &unary.node.expr, set);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_array_fors_in_expression(checker, &call.node.callee, set);
            for arg in &call.node.args {
                collect_array_fors_in_expression(checker, arg, set);
            }
        }
        HirExpressionNode::MemberExpression(member) => {
            collect_array_fors_in_expression(checker, &member.node.target, set);
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_array_fors_in_expression(checker, &match_expr.node.scrutinee, set);
            for arm in &match_expr.node.arms {
                collect_array_fors_in_expression(checker, &arm.node.value, set);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_array_fors_in_block(checker, &block_expr.node.block, set);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_array_fors_in_expression(checker, &grouped.node.expr, set);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_array_fors_in_expression(checker, &assign.node.target, set);
            collect_array_fors_in_expression(checker, &assign.node.value, set);
        }
        HirExpressionNode::IndexExpression(index_expr) => {
            collect_array_fors_in_expression(checker, &index_expr.node.target, set);
            collect_array_fors_in_expression(checker, &index_expr.node.index, set);
        }
        HirExpressionNode::ArrayLiteralExpression(lit) => {
            for element in &lit.node.elements {
                collect_array_fors_in_expression(checker, element, set);
            }
        }
        HirExpressionNode::EnumConstructorExpression(constructor) => {
            for arg in &constructor.node.args {
                collect_array_fors_in_expression(checker, arg, set);
            }
        }
        HirExpressionNode::StructLiteralExpression(struct_lit) => {
            for field in &struct_lit.node.fields {
                collect_array_fors_in_expression(checker, &field.node.value, set);
            }
        }
        HirExpressionNode::LambdaExpression(lambda) => {
            collect_array_fors_in_expression(checker, &lambda.node.body, set);
        }
        _ => {}
    }
}
