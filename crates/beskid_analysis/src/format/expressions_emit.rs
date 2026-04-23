use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{
    AssignExpression, AssignOp, BinaryExpression, BinaryOp, BlockExpression, CallExpression,
    EnumConstructorExpression, EnumPattern, Expression, GroupedExpression, LambdaExpression,
    LambdaParameter, Literal, LiteralExpression, MatchArm, MatchExpression, MemberExpression,
    PathExpression, Pattern, Spanned, StructLiteralExpression, StructLiteralField, TryExpression,
    UnaryExpression, UnaryOp,
};
use std::fmt::Write;

impl Emit for Literal {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Literal::Integer(s) | Literal::Float(s) | Literal::String(s) | Literal::Char(s) => {
                w.write_str(s)?;
            }
            Literal::Bool(true) => w.write_str("true")?,
            Literal::Bool(false) => w.write_str("false")?,
        }
        Ok(())
    }
}

impl Emit for Spanned<Literal> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for LiteralExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.literal.emit(w, cx)
    }
}

impl Emit for Spanned<LiteralExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for PathExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.path.emit(w, cx)
    }
}

impl Emit for Spanned<PathExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for UnaryOp {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_str(match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        })?;
        Ok(())
    }
}

impl Emit for Spanned<UnaryOp> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for UnaryExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.op.emit(w, cx)?;
        self.expr.emit(w, cx)
    }
}

impl Emit for Spanned<UnaryExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for BinaryOp {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let s = match self {
            BinaryOp::Or => "||",
            BinaryOp::And => "&&",
            BinaryOp::IdentityEq => "===",
            BinaryOp::IdentityNotEq => "!==",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Lte => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Gte => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
        };
        cx.token(w, s)
    }
}

impl Emit for Spanned<BinaryOp> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for BinaryExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.left.emit(w, cx)?;
        cx.space(w)?;
        self.op.emit(w, cx)?;
        cx.space(w)?;
        self.right.emit(w, cx)
    }
}

impl Emit for Spanned<BinaryExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for AssignOp {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            AssignOp::Assign => cx.token(w, "="),
            AssignOp::AddAssign => cx.token(w, "+="),
            AssignOp::SubAssign => cx.token(w, "-="),
        }
    }
}

impl Emit for Spanned<AssignOp> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for AssignExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.target.emit(w, cx)?;
        cx.space(w)?;
        self.op.emit(w, cx)?;
        cx.space(w)?;
        self.value.emit(w, cx)
    }
}

impl Emit for Spanned<AssignExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for CallExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.callee.emit(w, cx)?;
        w.write_char('(')?;
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            a.emit(w, cx)?;
        }
        w.write_char(')')?;
        Ok(())
    }
}

impl Emit for Spanned<CallExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MemberExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.target.emit(w, cx)?;
        w.write_char('.')?;
        self.member.emit(w, cx)
    }
}

impl Emit for Spanned<MemberExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for GroupedExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_char('(')?;
        self.expr.emit(w, cx)?;
        w.write_char(')')?;
        Ok(())
    }
}

impl Emit for Spanned<GroupedExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for TryExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.expr.emit(w, cx)?;
        w.write_char('?')?;
        Ok(())
    }
}

impl Emit for Spanned<TryExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for BlockExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.block.emit(w, cx)
    }
}

impl Emit for Spanned<BlockExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for LambdaParameter {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if let Some(ty) = &self.ty {
            ty.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)
    }
}

impl Emit for Spanned<LambdaParameter> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for LambdaExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if self.parameters.len() == 1 && self.parameters[0].node.ty.is_none() {
            self.parameters[0].emit(w, cx)?;
        } else {
            w.write_char('(')?;
            for (i, p) in self.parameters.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                p.emit(w, cx)?;
            }
            w.write_char(')')?;
        }
        cx.space(w)?;
        cx.token(w, "=>")?;
        cx.space(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<LambdaExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for EnumPattern {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.path.emit(w, cx)?;
        if !self.items.is_empty() {
            w.write_char('(')?;
            for (i, p) in self.items.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                p.emit(w, cx)?;
            }
            w.write_char(')')?;
        }
        Ok(())
    }
}

impl Emit for Spanned<EnumPattern> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Pattern {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Pattern::Wildcard => w.write_str("_")?,
            Pattern::Identifier(i) => i.emit(w, cx)?,
            Pattern::Literal(l) => l.emit(w, cx)?,
            Pattern::Enum(e) => e.emit(w, cx)?,
        }
        Ok(())
    }
}

impl Emit for Spanned<Pattern> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MatchArm {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.pattern.emit(w, cx)?;
        if let Some(g) = &self.guard {
            cx.space(w)?;
            cx.token(w, "when")?;
            cx.space(w)?;
            g.emit(w, cx)?;
        }
        cx.space(w)?;
        cx.token(w, "=>")?;
        cx.space(w)?;
        self.value.emit(w, cx)
    }
}

impl Emit for Spanned<MatchArm> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MatchExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "match")?;
        cx.space(w)?;
        self.scrutinee.emit(w, cx)?;
        if self.arms.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, arm) in self.arms.iter().enumerate() {
            if i > 0 {
                cx.nl(w)?;
            }
            cx.write_indent(w)?;
            arm.emit(w, cx)?;
            cx.token(w, ",")?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<MatchExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for StructLiteralField {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        cx.token(w, ": ")?;
        self.value.emit(w, cx)
    }
}

impl Emit for Spanned<StructLiteralField> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for StructLiteralExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.path.emit(w, cx)?;
        if self.fields.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, f) in self.fields.iter().enumerate() {
            if i > 0 {
                cx.nl(w)?;
            }
            cx.write_indent(w)?;
            f.emit(w, cx)?;
            cx.token(w, ",")?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<StructLiteralExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for EnumConstructorExpression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.path.emit(w, cx)?;
        w.write_char('(')?;
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            a.emit(w, cx)?;
        }
        w.write_char(')')?;
        Ok(())
    }
}

impl Emit for Spanned<EnumConstructorExpression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Expression {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Expression::Match(m) => m.emit(w, cx),
            Expression::Lambda(l) => l.emit(w, cx),
            Expression::Assign(a) => a.emit(w, cx),
            Expression::Binary(b) => b.emit(w, cx),
            Expression::Unary(u) => u.emit(w, cx),
            Expression::Call(c) => c.emit(w, cx),
            Expression::Member(m) => m.emit(w, cx),
            Expression::Literal(l) => l.emit(w, cx),
            Expression::Path(p) => p.emit(w, cx),
            Expression::StructLiteral(s) => s.emit(w, cx),
            Expression::EnumConstructor(e) => e.emit(w, cx),
            Expression::Block(b) => b.emit(w, cx),
            Expression::Grouped(g) => g.emit(w, cx),
            Expression::Try(t) => t.emit(w, cx),
        }
    }
}

impl Emit for Spanned<Expression> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
