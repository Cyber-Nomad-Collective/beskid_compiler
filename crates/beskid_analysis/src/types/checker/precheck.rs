//! Pre-normalize type queries using merged unit surfaces (try `?` targets, array for-loops).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::resolve::Resolution;
use crate::syntax::{Expression, Node, Program, Statement};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::TypeInfo;
use crate::types::surface::{build_unit_type_surface, merge_unit_surfaces_with_types};

use super::TypeChecker;

/// Resolved enum used when lowering `expr?` to `match`.
#[derive(Debug, Clone)]
pub struct TryDesugarTarget {
    pub type_name: String,
    pub ok_variant: String,
}

const PRECHECK_ROOT: &str = "__precheck";

/// Build a [`TypeChecker`] seeded from merged surfaces for the given programs (last = entry).
pub(crate) fn precheck_checker<'a>(resolution: &'a Resolution, programs: &[&'a Spanned<Program>]) -> TypeChecker<'a> {
    let Some(entry) = programs.last().copied() else {
        return TypeChecker::new(resolution, &Default::default());
    };

    let root = Path::new(PRECHECK_ROOT);
    let entry_path = root.join("entry");
    let entry_surface = Arc::new(build_unit_type_surface(entry, resolution, &entry_path));

    let dependency_surfaces =
        programs[..programs.len().saturating_sub(1)].iter().enumerate().map(|(index, program)| {
            let path = root.join(format!("dep_{index}"));
            (path.clone(), Arc::new(build_unit_type_surface(program, resolution, &path)))
        });

    let (merged_types, merged) = merge_unit_surfaces_with_types(dependency_surfaces, entry_surface);
    let mut checker = TypeChecker::from_merged(resolution, &merged, merged_types);
    for program in programs {
        checker.seed_program_enums(program);
    }
    checker
}

impl<'a> TypeChecker<'a> {
    /// Infer `Result`-shaped enum metadata for a `?` operand.
    pub fn try_desugar_target_for_operand(&mut self, operand: &Spanned<Expression>) -> Option<TryDesugarTarget> {
        let target_type = self.infer_expression_type(operand)?;
        let item_id = self.item_for_type_id(target_type)?;
        let type_name = self.resolution.items.iter().find(|info| info.id == item_id).map(|info| info.name.clone())?;
        let ok_variant = self.variant_display_name(item_id, "Ok")?;
        Some(TryDesugarTarget { type_name, ok_variant })
    }

    /// True when a `?` operand is a Result-shaped enum: it declares both an `Ok` and an `Error`
    /// variant. An enum that only happens to declare `Ok` is not a try target. Delegates to the
    /// single shared predicate ([`TypeChecker::is_result_shaped_enum`]) so this precheck and the
    /// full type checker's lower-spine check can never disagree.
    fn is_result_shaped_try_operand(&mut self, operand: &Spanned<Expression>) -> bool {
        let Some(target_type) = self.infer_expression_type(operand) else {
            return false;
        };
        let Some(item_id) = self.item_for_type_id(target_type) else {
            return false;
        };
        self.is_result_shaped_enum(item_id)
    }

    /// True when the iterable expression type is `T[]`.
    pub fn is_array_iterable(&mut self, iterable: &Spanned<Expression>) -> bool {
        let Some(target_type) = self.infer_expression_type(iterable) else {
            return false;
        };
        matches!(self.type_table.get(target_type), Some(TypeInfo::Array(_)))
    }

    /// Spans of `?` operands that are not a `Result`-shaped enum.
    pub fn invalid_try_expression_spans(resolution: &'a Resolution, entry: &Spanned<Program>) -> Vec<SpanInfo> {
        let programs: Vec<&Spanned<Program>> = vec![entry];
        let mut checker = precheck_checker(resolution, &programs);
        // A `?` operand is usually a parameter or `let` binding. Its type is only known once
        // the enclosing body has been typed, so type the entry items exactly as `check_entry`
        // does before judging operands. Body typing errors belong to the lower spine and are
        // discarded here; an operand whose type still cannot be established stays invalid.
        checker.seed_struct_definitions(entry);
        checker.seed_generics_from_program(entry);
        checker.seed_contract_signatures(entry);
        checker.seed_method_receivers_from_items(&entry.node.items);
        for item in &entry.node.items {
            checker.type_item(item);
        }
        let mut spans = Vec::new();
        collect_invalid_try_targets(&mut checker, entry, &mut spans);
        spans
    }

    /// Map try-expression span → desugar metadata (computed before in-place normalization).
    pub fn try_desugar_targets_for_program(
        resolution: &'a Resolution,
        entry: &Spanned<Program>,
        dependency_programs: &[&Spanned<Program>],
    ) -> HashMap<SpanInfo, TryDesugarTarget> {
        let mut programs: Vec<&Spanned<Program>> = dependency_programs.to_vec();
        programs.push(entry);
        let mut checker = precheck_checker(resolution, &programs);
        // `precheck_checker` only seeds declaration surfaces (types/enums), never types function
        // bodies. A `?` operand is frequently a `let`-bound local, whose type is only known once
        // its enclosing body has been typed — so type the entry items the same way
        // `invalid_try_expression_spans` does before walking for desugar targets. Without this,
        // `infer_expression_type` on a local operand returns `None` and the target is silently
        // dropped instead of desugared.
        checker.seed_struct_definitions(entry);
        checker.seed_generics_from_program(entry);
        checker.seed_contract_signatures(entry);
        checker.seed_method_receivers_from_items(&entry.node.items);
        for item in &entry.node.items {
            checker.type_item(item);
        }
        let mut map = HashMap::new();
        collect_try_targets(&mut checker, entry, &mut map);
        map
    }

    /// Map for-statement span → true when the iterable type is `T[]`.
    pub fn collect_array_for_spans(
        resolution: &'a Resolution,
        entry: &Spanned<Program>,
        dependency_programs: &[&Spanned<Program>],
    ) -> HashSet<SpanInfo> {
        let mut programs: Vec<&Spanned<Program>> = dependency_programs.to_vec();
        programs.push(entry);
        let mut checker = precheck_checker(resolution, &programs);
        let mut set = HashSet::new();
        collect_array_fors(&mut checker, entry, &mut set);
        set
    }
}

fn collect_invalid_try_targets(checker: &mut TypeChecker<'_>, program: &Spanned<Program>, spans: &mut Vec<SpanInfo>) {
    for item in &program.node.items {
        collect_invalid_try_targets_item(checker, item, spans);
    }
}

fn collect_invalid_try_targets_item(checker: &mut TypeChecker<'_>, item: &Spanned<Node>, spans: &mut Vec<SpanInfo>) {
    match &item.node {
        Node::Function(def) => {
            collect_invalid_try_targets_in_block(checker, &def.node.body, spans);
        }
        Node::Method(def) => {
            collect_invalid_try_targets_in_block(checker, &def.node.body, spans);
        }
        Node::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_invalid_try_targets_item(checker, nested, spans);
            }
        }
        _ => {}
    }
}

fn collect_invalid_try_targets_in_block(
    checker: &mut TypeChecker<'_>,
    block: &Spanned<crate::syntax::Block>,
    spans: &mut Vec<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let Statement::Use(scoped) = &statement.node
            && let Some(body) = &scoped.node.body
        {
            collect_invalid_try_targets_in_block(checker, body, spans);
        }
        if let Statement::Expression(expr_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(checker, &expr_stmt.node.expression, spans);
        } else if let Statement::Let(let_stmt)
        | Statement::Use(Spanned {
            node: crate::syntax::ScopedUseStatement { binding: let_stmt, .. },
            ..
        }) = &statement.node
        {
            collect_invalid_try_targets_in_expression(checker, &let_stmt.node.value, spans);
        } else if let Statement::Return(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
            collect_invalid_try_targets_in_expression(checker, value, spans);
        } else if let Statement::While(while_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(checker, &while_stmt.node.condition, spans);
            collect_invalid_try_targets_in_block(checker, &while_stmt.node.body, spans);
        } else if let Statement::For(for_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(checker, &for_stmt.node.iterable, spans);
            collect_invalid_try_targets_in_block(checker, &for_stmt.node.body, spans);
        } else if let Statement::If(if_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(checker, &if_stmt.node.condition, spans);
            collect_invalid_try_targets_in_block(checker, &if_stmt.node.then_block, spans);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_invalid_try_targets_in_else_branch(checker, else_branch, spans);
            }
        } else if let Statement::With(with_stmt) = &statement.node {
            for arg in &with_stmt.node.arguments {
                collect_invalid_try_targets_in_expression(checker, arg, spans);
            }
            collect_invalid_try_targets_in_block(checker, &with_stmt.node.body, spans);
        } else if let Statement::Launch(launch_stmt) = &statement.node {
            for arg in &launch_stmt.node.arguments {
                collect_invalid_try_targets_in_expression(checker, arg, spans);
            }
        }
    }
}

fn collect_invalid_try_targets_in_else_branch(
    checker: &mut TypeChecker<'_>,
    else_branch: &Spanned<crate::syntax::ElseBranch>,
    spans: &mut Vec<SpanInfo>,
) {
    match &else_branch.node {
        crate::syntax::ElseBranch::Block(block) => {
            collect_invalid_try_targets_in_block(checker, block, spans);
        }
        crate::syntax::ElseBranch::If(nested) => {
            collect_invalid_try_targets_in_expression(checker, &nested.node.condition, spans);
            collect_invalid_try_targets_in_block(checker, &nested.node.then_block, spans);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_invalid_try_targets_in_else_branch(checker, nested_else, spans);
            }
        }
    }
}

fn collect_invalid_try_targets_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<Expression>,
    spans: &mut Vec<SpanInfo>,
) {
    if let Expression::Try(try_expr) = &expr.node
        && !checker.is_result_shaped_try_operand(&try_expr.node.expr)
    {
        // `try_expr.span`, not the outer `expr.span`: the parser assigns the inner
        // `Spanned<TryExpression>` the operand-only span (see `parse_postfix_expression` in
        // `syntax/expressions/expression.rs`) before widening the outer expression span to
        // include the `?` token. The lower-spine full check (`type_try_expression` in
        // `checker/expressions/dispatch.rs`) and the query authority
        // (`beskid_queries::entry::invalid_try_spans`, via the indexed `TryExpression` node)
        // both report that same inner span for the same operator, so this early diagnostic must
        // match it byte-for-byte or `dedupe_diagnostics` (item 2) cannot merge them.
        spans.push(try_expr.span);
    }
    match &expr.node {
        Expression::Binary(binary) => {
            collect_invalid_try_targets_in_expression(checker, &binary.node.left, spans);
            collect_invalid_try_targets_in_expression(checker, &binary.node.right, spans);
        }
        Expression::Unary(unary) => {
            collect_invalid_try_targets_in_expression(checker, &unary.node.expr, spans);
        }
        Expression::Call(call) => {
            collect_invalid_try_targets_in_expression(checker, &call.node.callee, spans);
            for arg in &call.node.args {
                collect_invalid_try_targets_in_expression(checker, arg, spans);
            }
        }
        Expression::Match(match_expr) => {
            collect_invalid_try_targets_in_expression(checker, &match_expr.node.scrutinee, spans);
            for arm in &match_expr.node.arms {
                collect_invalid_try_targets_in_expression(checker, &arm.node.value, spans);
            }
        }
        Expression::Block(block_expr) => {
            collect_invalid_try_targets_in_block(checker, &block_expr.node.block, spans);
        }
        Expression::Grouped(grouped) => {
            collect_invalid_try_targets_in_expression(checker, &grouped.node.expr, spans);
        }
        Expression::Assign(assign) => {
            collect_invalid_try_targets_in_expression(checker, &assign.node.target, spans);
            collect_invalid_try_targets_in_expression(checker, &assign.node.value, spans);
        }
        _ => {}
    }
}

fn collect_try_targets(
    checker: &mut TypeChecker<'_>,
    program: &Spanned<Program>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for item in &program.node.items {
        collect_try_targets_item(checker, item, map);
    }
}

fn collect_try_targets_item(
    checker: &mut TypeChecker<'_>,
    item: &Spanned<Node>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    match &item.node {
        Node::Function(def) => {
            collect_try_targets_in_block(checker, &def.node.body, map);
        }
        Node::Method(def) => {
            collect_try_targets_in_block(checker, &def.node.body, map);
        }
        Node::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_try_targets_item(checker, nested, map);
            }
        }
        _ => {}
    }
}

fn collect_try_targets_in_block(
    checker: &mut TypeChecker<'_>,
    block: &Spanned<crate::syntax::Block>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for statement in &block.node.statements {
        if let Statement::Use(scoped) = &statement.node
            && let Some(body) = &scoped.node.body
        {
            collect_try_targets_in_block(checker, body, map);
        }
        if let Statement::Expression(expr_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &expr_stmt.node.expression, map);
        } else if let Statement::Let(let_stmt)
        | Statement::Use(Spanned {
            node: crate::syntax::ScopedUseStatement { binding: let_stmt, .. },
            ..
        }) = &statement.node
        {
            collect_try_targets_in_expression(checker, &let_stmt.node.value, map);
        } else if let Statement::Return(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
            collect_try_targets_in_expression(checker, value, map);
        } else if let Statement::While(while_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &while_stmt.node.condition, map);
            collect_try_targets_in_block(checker, &while_stmt.node.body, map);
        } else if let Statement::For(for_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &for_stmt.node.iterable, map);
            collect_try_targets_in_block(checker, &for_stmt.node.body, map);
        } else if let Statement::If(if_stmt) = &statement.node {
            collect_try_targets_in_expression(checker, &if_stmt.node.condition, map);
            collect_try_targets_in_block(checker, &if_stmt.node.then_block, map);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_try_targets_in_else_branch(checker, else_branch, map);
            }
        } else if let Statement::With(with_stmt) = &statement.node {
            for arg in &with_stmt.node.arguments {
                collect_try_targets_in_expression(checker, arg, map);
            }
            collect_try_targets_in_block(checker, &with_stmt.node.body, map);
        } else if let Statement::Launch(launch_stmt) = &statement.node {
            for arg in &launch_stmt.node.arguments {
                collect_try_targets_in_expression(checker, arg, map);
            }
        }
    }
}

fn collect_try_targets_in_else_branch(
    checker: &mut TypeChecker<'_>,
    else_branch: &Spanned<crate::syntax::ElseBranch>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    match &else_branch.node {
        crate::syntax::ElseBranch::Block(block) => {
            collect_try_targets_in_block(checker, block, map);
        }
        crate::syntax::ElseBranch::If(nested) => {
            collect_try_targets_in_expression(checker, &nested.node.condition, map);
            collect_try_targets_in_block(checker, &nested.node.then_block, map);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_try_targets_in_else_branch(checker, nested_else, map);
            }
        }
    }
}

fn collect_try_targets_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<Expression>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    if let Expression::Try(try_expr) = &expr.node
        && let Some(target) = checker.try_desugar_target_for_operand(&try_expr.node.expr)
    {
        // `try_expr.span`, matching `collect_invalid_try_targets_in_expression` and the
        // lower-spine/authority sites — see the comment there.
        map.insert(try_expr.span, target);
    }
    match &expr.node {
        Expression::Binary(binary) => {
            collect_try_targets_in_expression(checker, &binary.node.left, map);
            collect_try_targets_in_expression(checker, &binary.node.right, map);
        }
        Expression::Unary(unary) => {
            collect_try_targets_in_expression(checker, &unary.node.expr, map);
        }
        Expression::Call(call) => {
            collect_try_targets_in_expression(checker, &call.node.callee, map);
            for arg in &call.node.args {
                collect_try_targets_in_expression(checker, arg, map);
            }
        }
        Expression::Match(match_expr) => {
            collect_try_targets_in_expression(checker, &match_expr.node.scrutinee, map);
            for arm in &match_expr.node.arms {
                collect_try_targets_in_expression(checker, &arm.node.value, map);
            }
        }
        Expression::Block(block_expr) => {
            collect_try_targets_in_block(checker, &block_expr.node.block, map);
        }
        Expression::Grouped(grouped) => {
            collect_try_targets_in_expression(checker, &grouped.node.expr, map);
        }
        Expression::Assign(assign) => {
            collect_try_targets_in_expression(checker, &assign.node.target, map);
            collect_try_targets_in_expression(checker, &assign.node.value, map);
        }
        _ => {}
    }
}

fn collect_array_fors(checker: &mut TypeChecker<'_>, program: &Spanned<Program>, set: &mut HashSet<SpanInfo>) {
    for item in &program.node.items {
        collect_array_fors_item(checker, item, set);
    }
}

fn collect_array_fors_item(checker: &mut TypeChecker<'_>, item: &Spanned<Node>, set: &mut HashSet<SpanInfo>) {
    match &item.node {
        Node::Function(def) => {
            collect_array_fors_in_block(checker, &def.node.body, set);
        }
        Node::Method(def) => {
            collect_array_fors_in_block(checker, &def.node.body, set);
        }
        Node::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_array_fors_item(checker, nested, set);
            }
        }
        _ => {}
    }
}

fn collect_array_fors_in_else_branch(
    checker: &mut TypeChecker<'_>,
    else_branch: &Spanned<crate::syntax::ElseBranch>,
    set: &mut HashSet<SpanInfo>,
) {
    match &else_branch.node {
        crate::syntax::ElseBranch::Block(block) => {
            collect_array_fors_in_block(checker, block, set);
        }
        crate::syntax::ElseBranch::If(nested) => {
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
    block: &Spanned<crate::syntax::Block>,
    set: &mut HashSet<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let Statement::For(for_stmt) = &statement.node
            && checker.is_array_iterable(&for_stmt.node.iterable)
        {
            set.insert(statement.span);
        }
        collect_array_fors_in_statement(checker, statement, set);
    }
}

fn collect_array_fors_in_statement(
    checker: &mut TypeChecker<'_>,
    statement: &Spanned<Statement>,
    set: &mut HashSet<SpanInfo>,
) {
    if let Statement::Use(scoped) = &statement.node
        && let Some(body) = &scoped.node.body
    {
        collect_array_fors_in_block(checker, body, set);
    }
    match &statement.node {
        Statement::Let(let_stmt)
        | Statement::Use(Spanned { node: crate::syntax::ScopedUseStatement { binding: let_stmt, .. }, .. }) => {
            collect_array_fors_in_expression(checker, &let_stmt.node.value, set);
        }
        Statement::Return(ret) => {
            if let Some(value) = &ret.node.value {
                collect_array_fors_in_expression(checker, value, set);
            }
        }
        Statement::While(while_stmt) => {
            collect_array_fors_in_expression(checker, &while_stmt.node.condition, set);
            collect_array_fors_in_block(checker, &while_stmt.node.body, set);
        }
        Statement::If(if_stmt) => {
            collect_array_fors_in_expression(checker, &if_stmt.node.condition, set);
            collect_array_fors_in_block(checker, &if_stmt.node.then_block, set);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_array_fors_in_else_branch(checker, else_branch, set);
            }
        }
        Statement::Expression(expr_stmt) => {
            collect_array_fors_in_expression(checker, &expr_stmt.node.expression, set);
        }
        Statement::For(_) => {}
        _ => {}
    }
}

fn collect_array_fors_in_expression(
    checker: &mut TypeChecker<'_>,
    expr: &Spanned<Expression>,
    set: &mut HashSet<SpanInfo>,
) {
    match &expr.node {
        Expression::Binary(binary) => {
            collect_array_fors_in_expression(checker, &binary.node.left, set);
            collect_array_fors_in_expression(checker, &binary.node.right, set);
        }
        Expression::Unary(unary) => {
            collect_array_fors_in_expression(checker, &unary.node.expr, set);
        }
        Expression::Call(call) => {
            collect_array_fors_in_expression(checker, &call.node.callee, set);
            for arg in &call.node.args {
                collect_array_fors_in_expression(checker, arg, set);
            }
        }
        Expression::Member(member) => {
            collect_array_fors_in_expression(checker, &member.node.target, set);
        }
        Expression::Match(match_expr) => {
            collect_array_fors_in_expression(checker, &match_expr.node.scrutinee, set);
            for arm in &match_expr.node.arms {
                collect_array_fors_in_expression(checker, &arm.node.value, set);
            }
        }
        Expression::Block(block_expr) => {
            collect_array_fors_in_block(checker, &block_expr.node.block, set);
        }
        Expression::Grouped(grouped) => {
            collect_array_fors_in_expression(checker, &grouped.node.expr, set);
        }
        Expression::Assign(assign) => {
            collect_array_fors_in_expression(checker, &assign.node.target, set);
            collect_array_fors_in_expression(checker, &assign.node.value, set);
        }
        Expression::Index(index_expr) => {
            collect_array_fors_in_expression(checker, &index_expr.node.target, set);
            collect_array_fors_in_expression(checker, &index_expr.node.index, set);
        }
        Expression::ArrayLiteral(lit) => {
            for element in &lit.node.elements {
                collect_array_fors_in_expression(checker, element, set);
            }
        }
        Expression::EnumConstructor(constructor) => {
            for arg in &constructor.node.args {
                collect_array_fors_in_expression(checker, arg, set);
            }
        }
        Expression::StructLiteral(struct_lit) => {
            for field in &struct_lit.node.fields {
                collect_array_fors_in_expression(checker, &field.node.value, set);
            }
        }
        Expression::Lambda(lambda) => {
            collect_array_fors_in_expression(checker, &lambda.node.body, set);
        }
        _ => {}
    }
}
