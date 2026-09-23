use crate::syntax::{Block, ElseBranch, IfStatement, Spanned, Statement};

use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn walk_block(&mut self, block: &Spanned<Block>) {
        for stmt in &block.node.statements {
            self.walk_statement(stmt);
        }
    }

    pub(super) fn walk_statement(&mut self, stmt: &Spanned<Statement>) {
        if let Statement::Use(scoped) = &stmt.node
            && let Some(body) = &scoped.node.body
        {
            self.walk_block(body);
        }
        match &stmt.node {
            Statement::Let(let_stmt)
            | Statement::Use(Spanned { node: crate::syntax::ScopedUseStatement { binding: let_stmt, .. }, .. }) => {
                if let Some(ty) = &let_stmt.node.type_annotation {
                    let expected = self.type_id_for_program_type(ty);
                    let prev = self.contextual_expected_type;
                    if let Some(e) = expected {
                        self.contextual_expected_type = Some(e);
                    }
                    self.walk_expression(&let_stmt.node.value);
                    self.contextual_expected_type = prev;
                    if let (Some(e), Some(a)) = (expected, self.expr_type(&let_stmt.node.value)) {
                        self.record_numeric_cast(let_stmt.node.value.id, let_stmt.node.name.span, e, a);
                    }
                } else {
                    self.walk_expression(&let_stmt.node.value);
                }
            }
            Statement::Return(ret) => {
                let prev = self.contextual_expected_type;
                if let Some(e) = self.current_return_type {
                    self.contextual_expected_type = Some(e);
                }
                if let Some(expr) = &ret.node.value {
                    self.walk_expression(expr);
                    if let (Some(e), Some(a)) = (self.current_return_type, self.expr_type(expr)) {
                        self.record_numeric_cast(ret.id, ret.span, e, a);
                    }
                }
                self.contextual_expected_type = prev;
            }
            Statement::While(w) => {
                self.walk_expression(&w.node.condition);
                self.walk_block(&w.node.body);
            }
            Statement::For(f) => {
                self.walk_expression(&f.node.iterable);
                self.walk_block(&f.node.body);
            }
            Statement::If(i) => self.walk_if(i),
            Statement::Expression(e) => self.walk_expression(&e.node.expression),
            _ => {}
        }
    }

    pub(super) fn walk_if(&mut self, if_stmt: &Spanned<IfStatement>) {
        self.walk_expression(&if_stmt.node.condition);
        self.walk_block(&if_stmt.node.then_block);
        if let Some(e) = &if_stmt.node.else_branch {
            match &e.node {
                ElseBranch::Block(b) => self.walk_block(b),
                ElseBranch::If(n) => self.walk_if(n),
            }
        }
    }
}
