//! Post-normalize HIR walk assigning stable [`HirNodeId`](crate::resolve::HirNodeId) values.

use crate::hir::{
    HirBlock, HirElseBranch, HirExpressionNode, HirItem, HirMatchArm, HirPattern, HirProgram, HirStatementNode,
};
use crate::resolve::HirNodeId;
use crate::syntax::Spanned;

struct IdGen(u32);

impl IdGen {
    fn next(&mut self) -> HirNodeId {
        self.0 += 1;
        HirNodeId(self.0)
    }
}

/// Assign dense ids to typable HIR nodes in pre-order.
pub fn index_program(program: &mut Spanned<HirProgram>) {
    let _ = index_program_from_base(program, 0);
}

/// Assign ids starting after `base` (last id already used). Returns the highest assigned id.
pub fn index_program_from_base(program: &mut Spanned<HirProgram>, base: u32) -> u32 {
    let mut r#gen = IdGen(base);
    for item in &mut program.node.items {
        index_item(item, &mut r#gen);
    }
    r#gen.0
}

/// Highest [`HirNodeId`] assigned anywhere in `program` (0 when none).
pub fn max_hir_node_id(program: &Spanned<HirProgram>) -> u32 {
    let mut max = 0u32;
    for item in &program.node.items {
        max = max_hir_node_id_item(item, max);
    }
    max
}

fn max_hir_node_id_item(item: &Spanned<HirItem>, mut max: u32) -> u32 {
    max = max.max(item.id.0);
    match &item.node {
        crate::hir::HirItem::FunctionDefinition(def) => max_hir_node_id_block(&def.node.body, max),
        crate::hir::HirItem::MethodDefinition(def) => max_hir_node_id_block(&def.node.body, max),
        crate::hir::HirItem::TestDefinition(def) => max_hir_node_id_block(&def.node.body, max),
        crate::hir::HirItem::ExtendTypeDefinition(def) => {
            for method in &def.node.methods {
                max = max.max(method.id.0);
                max = max_hir_node_id_block(&method.node.body, max);
            }
            max
        }
        crate::hir::HirItem::TypeDefinition(def) => {
            for method in &def.node.methods {
                max = max.max(method.id.0);
                max = max_hir_node_id_block(&method.node.body, max);
            }
            max
        }
        crate::hir::HirItem::InlineModule(m) => {
            for nested in &m.node.items {
                max = max_hir_node_id_item(nested, max);
            }
            max
        }
        _ => max,
    }
}

fn max_hir_node_id_block(block: &Spanned<HirBlock>, mut max: u32) -> u32 {
    max = max.max(block.id.0);
    for stmt in &block.node.statements {
        max = max_hir_node_id_statement(stmt, max);
    }
    max
}

fn max_hir_node_id_statement(stmt: &Spanned<HirStatementNode>, mut max: u32) -> u32 {
    max = max.max(stmt.id.0);
    match &stmt.node {
        HirStatementNode::LetStatement(let_stmt) => max_hir_node_id_expression(&let_stmt.node.value, max),
        HirStatementNode::ReturnStatement(ret) => {
            ret.node.value.as_ref().map(|expr| max_hir_node_id_expression(expr, max)).unwrap_or(max)
        }
        HirStatementNode::WhileStatement(w) => {
            max = max_hir_node_id_expression(&w.node.condition, max);
            max_hir_node_id_block(&w.node.body, max)
        }
        HirStatementNode::ForStatement(f) => {
            max = max_hir_node_id_expression(&f.node.iterable, max);
            max_hir_node_id_block(&f.node.body, max)
        }
        HirStatementNode::IfStatement(i) => max_hir_node_id_if(i, max),
        HirStatementNode::ExpressionStatement(e) => max_hir_node_id_expression(&e.node.expression, max),
        _ => max,
    }
}

fn max_hir_node_id_if(if_stmt: &Spanned<crate::hir::HirIfStatement>, mut max: u32) -> u32 {
    max = max_hir_node_id_expression(&if_stmt.node.condition, max);
    max = max_hir_node_id_block(&if_stmt.node.then_block, max);
    if let Some(branch) = &if_stmt.node.else_branch {
        match &branch.node {
            HirElseBranch::Block(b) => max_hir_node_id_block(b, max),
            HirElseBranch::If(nested) => max_hir_node_id_if(nested, max),
        }
    } else {
        max
    }
}

fn max_hir_node_id_expression(expr: &Spanned<HirExpressionNode>, mut max: u32) -> u32 {
    max = max.max(expr.id.0);
    match &expr.node {
        HirExpressionNode::CallExpression(call) => {
            max = max_hir_node_id_expression(&call.node.callee, max);
            for arg in &call.node.args {
                max = max_hir_node_id_expression(arg, max);
            }
            max
        }
        HirExpressionNode::AssignExpression(a) => {
            max = max_hir_node_id_expression(&a.node.target, max);
            max_hir_node_id_expression(&a.node.value, max)
        }
        HirExpressionNode::LambdaExpression(l) => max_hir_node_id_expression(&l.node.body, max),
        HirExpressionNode::StructLiteralExpression(lit) => {
            for field in &lit.node.fields {
                max = max_hir_node_id_expression(&field.node.value, max);
            }
            max
        }
        HirExpressionNode::EnumConstructorExpression(c) => {
            for arg in &c.node.args {
                max = max_hir_node_id_expression(arg, max);
            }
            max
        }
        HirExpressionNode::MatchExpression(m) => max_hir_node_id_match(m, max),
        HirExpressionNode::BinaryExpression(b) => {
            max = max_hir_node_id_expression(&b.node.left, max);
            max_hir_node_id_expression(&b.node.right, max)
        }
        HirExpressionNode::UnaryExpression(u) => max_hir_node_id_expression(&u.node.expr, max),
        HirExpressionNode::GroupedExpression(g) => max_hir_node_id_expression(&g.node.expr, max),
        HirExpressionNode::BlockExpression(b) => max_hir_node_id_block(&b.node.block, max),
        HirExpressionNode::MemberExpression(m) => max_hir_node_id_expression(&m.node.target, max),
        HirExpressionNode::IndexExpression(i) => {
            max = max_hir_node_id_expression(&i.node.target, max);
            max_hir_node_id_expression(&i.node.index, max)
        }
        HirExpressionNode::ArrayLiteralExpression(a) => {
            for e in &a.node.elements {
                max = max_hir_node_id_expression(e, max);
            }
            max
        }
        HirExpressionNode::TryExpression(t) => max_hir_node_id_expression(&t.node.expr, max),
        HirExpressionNode::SpawnExpression(s) => max_hir_node_id_expression(&s.node.callee, max),
        _ => max,
    }
}

fn max_hir_node_id_match(m: &Spanned<crate::hir::HirMatchExpression>, mut max: u32) -> u32 {
    max = max_hir_node_id_expression(&m.node.scrutinee, max);
    for arm in &m.node.arms {
        max = max.max(arm.id.0);
        max = max_hir_node_id_pattern(&arm.node.pattern, max);
        if let Some(guard) = &arm.node.guard {
            max = max_hir_node_id_expression(guard, max);
        }
        max = max_hir_node_id_expression(&arm.node.value, max);
    }
    max
}

fn max_hir_node_id_pattern(pattern: &Spanned<HirPattern>, mut max: u32) -> u32 {
    max = max.max(pattern.id.0);
    if let HirPattern::Enum(ep) = &pattern.node {
        for nested in &ep.node.items {
            max = max_hir_node_id_pattern(nested, max);
        }
    }
    max
}

fn assign_id<T>(node: &mut Spanned<T>, r#gen: &mut IdGen) {
    node.id = r#gen.next();
}

/// Clear and reassign ids across programs so merged [`TypeResult::node_types`] keys stay unique.
pub fn reindex_programs_in_place(programs: &mut [Spanned<HirProgram>]) {
    let mut base = 0u32;
    for program in programs {
        reset_program_node_ids(program);
        base = index_program_from_base(program, base);
    }
}

pub(crate) fn reset_program_node_ids(program: &mut Spanned<HirProgram>) {
    for item in &mut program.node.items {
        reset_item_node_ids(item);
    }
}

fn reset_item_node_ids(item: &mut Spanned<HirItem>) {
    item.id = HirNodeId::INVALID;
    match &mut item.node {
        crate::hir::HirItem::FunctionDefinition(def) => reset_block_node_ids(&mut def.node.body),
        crate::hir::HirItem::MethodDefinition(def) => reset_block_node_ids(&mut def.node.body),
        crate::hir::HirItem::TestDefinition(def) => reset_block_node_ids(&mut def.node.body),
        crate::hir::HirItem::ExtendTypeDefinition(def) => {
            for method in &mut def.node.methods {
                method.id = HirNodeId::INVALID;
                reset_block_node_ids(&mut method.node.body);
            }
        }
        crate::hir::HirItem::TypeDefinition(def) => {
            for method in &mut def.node.methods {
                method.id = HirNodeId::INVALID;
                reset_block_node_ids(&mut method.node.body);
            }
        }
        crate::hir::HirItem::InlineModule(m) => {
            for nested in &mut m.node.items {
                reset_item_node_ids(nested);
            }
        }
        _ => {}
    }
}

fn reset_block_node_ids(block: &mut Spanned<HirBlock>) {
    block.id = HirNodeId::INVALID;
    for stmt in &mut block.node.statements {
        reset_statement_node_ids(stmt);
    }
}

fn reset_statement_node_ids(stmt: &mut Spanned<HirStatementNode>) {
    stmt.id = HirNodeId::INVALID;
    match &mut stmt.node {
        HirStatementNode::LetStatement(let_stmt) => reset_expression_node_ids(&mut let_stmt.node.value),
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(expr) = &mut ret.node.value {
                reset_expression_node_ids(expr);
            }
        }
        HirStatementNode::WhileStatement(w) => {
            reset_expression_node_ids(&mut w.node.condition);
            reset_block_node_ids(&mut w.node.body);
        }
        HirStatementNode::ForStatement(f) => {
            reset_expression_node_ids(&mut f.node.iterable);
            reset_block_node_ids(&mut f.node.body);
        }
        HirStatementNode::IfStatement(i) => reset_if_node_ids(i),
        HirStatementNode::ExpressionStatement(e) => {
            reset_expression_node_ids(&mut e.node.expression);
        }
        _ => {}
    }
}

fn reset_if_node_ids(if_stmt: &mut Spanned<crate::hir::HirIfStatement>) {
    reset_expression_node_ids(&mut if_stmt.node.condition);
    reset_block_node_ids(&mut if_stmt.node.then_block);
    if let Some(branch) = &mut if_stmt.node.else_branch {
        match &mut branch.node {
            HirElseBranch::Block(b) => reset_block_node_ids(b),
            HirElseBranch::If(nested) => reset_if_node_ids(nested),
        }
    }
}

fn reset_expression_node_ids(expr: &mut Spanned<HirExpressionNode>) {
    expr.id = HirNodeId::INVALID;
    sync_wrapper_expression_id(expr);
    match &mut expr.node {
        HirExpressionNode::CallExpression(call) => {
            reset_expression_node_ids(&mut call.node.callee);
            for arg in &mut call.node.args {
                reset_expression_node_ids(arg);
            }
        }
        HirExpressionNode::AssignExpression(a) => {
            reset_expression_node_ids(&mut a.node.target);
            reset_expression_node_ids(&mut a.node.value);
        }
        HirExpressionNode::LambdaExpression(l) => reset_expression_node_ids(&mut l.node.body),
        HirExpressionNode::StructLiteralExpression(lit) => {
            for field in &mut lit.node.fields {
                reset_expression_node_ids(&mut field.node.value);
            }
        }
        HirExpressionNode::EnumConstructorExpression(c) => {
            for arg in &mut c.node.args {
                reset_expression_node_ids(arg);
            }
        }
        HirExpressionNode::MatchExpression(m) => reset_match_node_ids(m),
        HirExpressionNode::BinaryExpression(b) => {
            reset_expression_node_ids(&mut b.node.left);
            reset_expression_node_ids(&mut b.node.right);
        }
        HirExpressionNode::UnaryExpression(u) => reset_expression_node_ids(&mut u.node.expr),
        HirExpressionNode::GroupedExpression(g) => reset_expression_node_ids(&mut g.node.expr),
        HirExpressionNode::BlockExpression(b) => reset_block_node_ids(&mut b.node.block),
        HirExpressionNode::MemberExpression(m) => reset_expression_node_ids(&mut m.node.target),
        HirExpressionNode::IndexExpression(i) => {
            reset_expression_node_ids(&mut i.node.target);
            reset_expression_node_ids(&mut i.node.index);
        }
        HirExpressionNode::ArrayLiteralExpression(a) => {
            for e in &mut a.node.elements {
                reset_expression_node_ids(e);
            }
        }
        HirExpressionNode::TryExpression(t) => reset_expression_node_ids(&mut t.node.expr),
        HirExpressionNode::SpawnExpression(s) => reset_expression_node_ids(&mut s.node.callee),
        _ => {}
    }
}

fn reset_match_node_ids(m: &mut Spanned<crate::hir::HirMatchExpression>) {
    reset_expression_node_ids(&mut m.node.scrutinee);
    for arm in &mut m.node.arms {
        arm.id = HirNodeId::INVALID;
        reset_pattern_node_ids(&mut arm.node.pattern);
        if let Some(guard) = &mut arm.node.guard {
            reset_expression_node_ids(guard);
        }
        reset_expression_node_ids(&mut arm.node.value);
    }
}

fn reset_pattern_node_ids(pattern: &mut Spanned<HirPattern>) {
    pattern.id = HirNodeId::INVALID;
    if let HirPattern::Enum(ep) = &mut pattern.node {
        for nested in &mut ep.node.items {
            reset_pattern_node_ids(nested);
        }
    }
}

fn index_method(method: &mut Spanned<crate::hir::HirMethodDefinition>, r#gen: &mut IdGen) {
    assign_id(method, r#gen);
    index_block(&mut method.node.body, r#gen);
}

fn index_item(item: &mut Spanned<HirItem>, r#gen: &mut IdGen) {
    assign_id(item, r#gen);
    match &mut item.node {
        crate::hir::HirItem::FunctionDefinition(def) => index_block(&mut def.node.body, r#gen),
        crate::hir::HirItem::MethodDefinition(def) => index_block(&mut def.node.body, r#gen),
        crate::hir::HirItem::TestDefinition(def) => index_block(&mut def.node.body, r#gen),
        crate::hir::HirItem::ExtendTypeDefinition(def) => {
            for method in &mut def.node.methods {
                index_method(method, r#gen);
            }
        }
        crate::hir::HirItem::TypeDefinition(def) => {
            for method in &mut def.node.methods {
                index_method(method, r#gen);
            }
        }
        crate::hir::HirItem::InlineModule(m) => {
            for nested in &mut m.node.items {
                index_item(nested, r#gen);
            }
        }
        _ => {}
    }
}

fn index_block(block: &mut Spanned<HirBlock>, r#gen: &mut IdGen) {
    assign_id(block, r#gen);
    for stmt in &mut block.node.statements {
        index_statement(stmt, r#gen);
    }
}

fn index_statement(stmt: &mut Spanned<HirStatementNode>, r#gen: &mut IdGen) {
    assign_id(stmt, r#gen);
    match &mut stmt.node {
        HirStatementNode::LetStatement(let_stmt) => {
            index_expression(&mut let_stmt.node.value, r#gen);
        }
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(expr) = &mut ret.node.value {
                index_expression(expr, r#gen);
            }
        }
        HirStatementNode::WhileStatement(w) => {
            index_expression(&mut w.node.condition, r#gen);
            index_block(&mut w.node.body, r#gen);
        }
        HirStatementNode::ForStatement(f) => {
            index_expression(&mut f.node.iterable, r#gen);
            index_block(&mut f.node.body, r#gen);
        }
        HirStatementNode::IfStatement(i) => index_if(i, r#gen),
        HirStatementNode::ExpressionStatement(e) => index_expression(&mut e.node.expression, r#gen),
        _ => {}
    }
}

fn index_if(if_stmt: &mut Spanned<crate::hir::HirIfStatement>, r#gen: &mut IdGen) {
    index_expression(&mut if_stmt.node.condition, r#gen);
    index_block(&mut if_stmt.node.then_block, r#gen);
    if let Some(branch) = &mut if_stmt.node.else_branch {
        match &mut branch.node {
            HirElseBranch::Block(b) => index_block(b, r#gen),
            HirElseBranch::If(nested) => index_if(nested, r#gen),
        }
    }
}

fn index_expression(expr: &mut Spanned<HirExpressionNode>, r#gen: &mut IdGen) {
    assign_id(expr, r#gen);
    sync_wrapper_expression_id(expr);
    match &mut expr.node {
        HirExpressionNode::CallExpression(call) => {
            index_expression(&mut call.node.callee, r#gen);
            for arg in &mut call.node.args {
                index_expression(arg, r#gen);
            }
        }
        HirExpressionNode::AssignExpression(a) => {
            index_expression(&mut a.node.target, r#gen);
            index_expression(&mut a.node.value, r#gen);
        }
        HirExpressionNode::LambdaExpression(l) => index_expression(&mut l.node.body, r#gen),
        HirExpressionNode::StructLiteralExpression(lit) => {
            for field in &mut lit.node.fields {
                index_expression(&mut field.node.value, r#gen);
            }
        }
        HirExpressionNode::EnumConstructorExpression(c) => {
            for arg in &mut c.node.args {
                index_expression(arg, r#gen);
            }
        }
        HirExpressionNode::MatchExpression(m) => index_match(m, r#gen),
        HirExpressionNode::BinaryExpression(b) => {
            index_expression(&mut b.node.left, r#gen);
            index_expression(&mut b.node.right, r#gen);
        }
        HirExpressionNode::UnaryExpression(u) => index_expression(&mut u.node.expr, r#gen),
        HirExpressionNode::GroupedExpression(g) => index_expression(&mut g.node.expr, r#gen),
        HirExpressionNode::BlockExpression(b) => index_block(&mut b.node.block, r#gen),
        HirExpressionNode::MemberExpression(m) => index_expression(&mut m.node.target, r#gen),
        HirExpressionNode::IndexExpression(i) => {
            index_expression(&mut i.node.target, r#gen);
            index_expression(&mut i.node.index, r#gen);
        }
        HirExpressionNode::ArrayLiteralExpression(a) => {
            for e in &mut a.node.elements {
                index_expression(e, r#gen);
            }
        }
        HirExpressionNode::TryExpression(t) => index_expression(&mut t.node.expr, r#gen),
        HirExpressionNode::SpawnExpression(s) => index_expression(&mut s.node.callee, r#gen),
        _ => {}
    }
}

fn index_match(m: &mut Spanned<crate::hir::HirMatchExpression>, r#gen: &mut IdGen) {
    index_expression(&mut m.node.scrutinee, r#gen);
    for arm in &mut m.node.arms {
        index_match_arm(arm, r#gen);
    }
}

fn index_match_arm(arm: &mut Spanned<HirMatchArm>, r#gen: &mut IdGen) {
    assign_id(arm, r#gen);
    index_pattern(&mut arm.node.pattern, r#gen);
    if let Some(guard) = &mut arm.node.guard {
        index_expression(guard, r#gen);
    }
    index_expression(&mut arm.node.value, r#gen);
}

fn index_pattern(pattern: &mut Spanned<HirPattern>, r#gen: &mut IdGen) {
    assign_id(pattern, r#gen);
    if let HirPattern::Enum(ep) = &mut pattern.node {
        for nested in &mut ep.node.items {
            index_pattern(nested, r#gen);
        }
    }
}

/// Inner [`Spanned`] wrappers (match, call, …) share the outer expression id so
/// [`TypeResult::node_types`] keys match codegen lookups on wrapper nodes.
fn sync_wrapper_expression_id(expr: &mut Spanned<HirExpressionNode>) {
    let id = expr.id;
    match &mut expr.node {
        HirExpressionNode::CallExpression(c) => c.id = id,
        HirExpressionNode::MatchExpression(m) => m.id = id,
        HirExpressionNode::BinaryExpression(b) => b.id = id,
        HirExpressionNode::UnaryExpression(u) => u.id = id,
        HirExpressionNode::MemberExpression(m) => m.id = id,
        HirExpressionNode::AssignExpression(a) => a.id = id,
        HirExpressionNode::LambdaExpression(l) => l.id = id,
        HirExpressionNode::StructLiteralExpression(s) => s.id = id,
        HirExpressionNode::EnumConstructorExpression(e) => e.id = id,
        HirExpressionNode::BlockExpression(b) => b.id = id,
        HirExpressionNode::GroupedExpression(g) => g.id = id,
        HirExpressionNode::IndexExpression(i) => i.id = id,
        HirExpressionNode::ArrayLiteralExpression(a) => a.id = id,
        HirExpressionNode::LiteralExpression(l) => l.id = id,
        HirExpressionNode::TryExpression(t) => t.id = id,
        HirExpressionNode::SpawnExpression(s) => s.id = id,
        _ => {}
    }
}
