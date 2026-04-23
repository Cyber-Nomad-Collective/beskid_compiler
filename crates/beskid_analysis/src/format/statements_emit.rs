use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{
    BreakStatement, ContinueStatement, Expression, ExpressionStatement, ForStatement, IfStatement,
    LetStatement, RangeExpression, ReturnStatement, Spanned, Statement, WhileStatement,
};
use std::fmt::Write;

fn emit_parenthesized_condition<W: Write>(
    condition: &Spanned<Expression>,
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    match &condition.node {
        // Keep idempotence: avoid wrapping already-grouped conditions again.
        Expression::Grouped(_) => condition.emit(w, cx),
        _ => {
            w.write_char('(')?;
            condition.emit(w, cx)?;
            w.write_char(')')?;
            Ok(())
        }
    }
}

impl Emit for LetStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if let Some(ty) = &self.type_annotation {
            ty.emit(w, cx)?;
            if self.mutable {
                cx.space(w)?;
                cx.token(w, "mut")?;
            }
            cx.space(w)?;
            self.name.emit(w, cx)?;
        } else {
            cx.token(w, "let")?;
            cx.space(w)?;
            self.name.emit(w, cx)?;
        }
        cx.space(w)?;
        cx.token(w, "=")?;
        cx.space(w)?;
        self.value.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<LetStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ReturnStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "return")?;
        if let Some(v) = &self.value {
            cx.space(w)?;
            v.emit(w, cx)?;
        }
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ReturnStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for BreakStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let _ = self;
        cx.token(w, "break;")
    }
}

impl Emit for Spanned<BreakStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContinueStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let _ = self;
        cx.token(w, "continue;")
    }
}

impl Emit for Spanned<ContinueStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for RangeExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "range")?;
        w.write_char('(')?;
        self.start.emit(w, cx)?;
        cx.token(w, ", ")?;
        self.end.emit(w, cx)?;
        w.write_char(')')?;
        Ok(())
    }
}

impl Emit for Spanned<RangeExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for WhileStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "while")?;
        cx.space(w)?;
        emit_parenthesized_condition(&self.condition, w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<WhileStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ForStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "for")?;
        cx.space(w)?;
        self.iterator.emit(w, cx)?;
        cx.space(w)?;
        cx.token(w, "in")?;
        cx.space(w)?;
        self.iterable.emit(w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<ForStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for IfStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "if")?;
        cx.space(w)?;
        emit_parenthesized_condition(&self.condition, w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.then_block.emit(w, cx)?;
        if let Some(else_b) = &self.else_block {
            cx.nl(w)?;
            cx.write_indent(w)?;
            cx.token(w, "else")?;
            cx.nl(w)?;
            cx.write_indent(w)?;
            else_b.emit(w, cx)?;
        }
        Ok(())
    }
}

impl Emit for Spanned<IfStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ExpressionStatement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.expression.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ExpressionStatement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Statement {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Statement::Let(s) => s.emit(w, cx),
            Statement::Return(s) => s.emit(w, cx),
            Statement::Break(s) => s.emit(w, cx),
            Statement::Continue(s) => s.emit(w, cx),
            Statement::While(s) => s.emit(w, cx),
            Statement::For(s) => s.emit(w, cx),
            Statement::If(s) => s.emit(w, cx),
            Statement::Expression(s) => s.emit(w, cx),
        }
    }
}

impl Emit for Spanned<Statement> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
