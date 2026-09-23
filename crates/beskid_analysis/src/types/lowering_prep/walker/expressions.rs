use crate::syntax::{Expression, Spanned};
use crate::types::TypeInfo;

use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn walk_expression(&mut self, expr: &Spanned<Expression>) {
        match &expr.node {
            Expression::Call(call) => {
                self.prep_call(expr.id, call);
                self.walk_expression(&call.node.callee);
                for a in &call.node.args {
                    self.walk_expression(a);
                }
            }
            Expression::Assign(a) => {
                self.walk_expression(&a.node.target);
                self.walk_expression(&a.node.value);
                if let (Some(t), Some(v)) = (self.expr_type(&a.node.target), self.expr_type(&a.node.value)) {
                    self.record_numeric_cast(expr.id, expr.span, t, v);
                }
            }
            Expression::Lambda(l) => {
                let sig = self.contextual_expected_type.and_then(|id| match self.surfaces.types.get(id)? {
                    TypeInfo::Function { params, return_type } => Some((params.clone(), *return_type)),
                    _ => None,
                });
                self.walk_expression(&l.node.body);
                if let (Some((_, er)), Some(rt)) = (sig.as_ref(), self.expr_type(&l.node.body)) {
                    self.record_numeric_cast(l.node.body.id, l.node.body.span, *er, rt);
                }
            }
            Expression::StructLiteral(lit) => {
                self.prep_struct_literal_casts(expr.id, lit);
                for f in &lit.node.fields {
                    self.walk_expression(&f.node.value);
                }
            }
            Expression::EnumConstructor(c) => {
                self.prep_enum_ctor_casts(c);
                for a in &c.node.args {
                    self.walk_expression(a);
                }
            }
            Expression::Match(m) => self.prep_match(m),
            Expression::Binary(b) => {
                self.walk_expression(&b.node.left);
                self.walk_expression(&b.node.right);
            }
            Expression::Unary(u) => self.walk_expression(&u.node.expr),
            Expression::Grouped(g) => self.walk_expression(&g.node.expr),
            Expression::Block(b) => self.walk_block(&b.node.block),
            Expression::Member(m) => self.walk_expression(&m.node.target),
            Expression::Index(i) => {
                self.walk_expression(&i.node.target);
                self.walk_expression(&i.node.index);
            }
            Expression::ArrayLiteral(a) => {
                for e in &a.node.elements {
                    self.walk_expression(e);
                }
            }
            Expression::Try(t) => self.walk_expression(&t.node.expr),
            Expression::Spawn(s) => self.walk_expression(&s.node.callee),
            _ => {}
        }
    }
}
