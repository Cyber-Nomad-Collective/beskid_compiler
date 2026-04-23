use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{
    Attribute, AttributeArgument, AttributeDeclaration, AttributeParameter, AttributeTarget,
    Spanned,
};
use std::fmt::Write;

impl Emit for AttributeArgument {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        cx.token(w, ": ")?;
        self.value.emit(w, cx)
    }
}

impl Emit for Spanned<AttributeArgument> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Attribute {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_char('[')?;
        self.name.emit(w, cx)?;
        if !self.arguments.is_empty() {
            w.write_char('(')?;
            for (i, a) in self.arguments.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                a.emit(w, cx)?;
            }
            w.write_char(')')?;
        }
        w.write_char(']')?;
        Ok(())
    }
}

impl Emit for Spanned<Attribute> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for AttributeTarget {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)
    }
}

impl Emit for Spanned<AttributeTarget> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for AttributeParameter {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        cx.token(w, ": ")?;
        self.ty.emit(w, cx)?;
        if let Some(d) = &self.default_value {
            cx.space(w)?;
            cx.token(w, "=")?;
            cx.space(w)?;
            d.emit(w, cx)?;
        }
        Ok(())
    }
}

impl Emit for Spanned<AttributeParameter> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for AttributeDeclaration {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        cx.token(w, "attribute")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        if !self.targets.is_empty() {
            w.write_char('(')?;
            for (i, t) in self.targets.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                t.emit(w, cx)?;
            }
            w.write_char(')')?;
        }
        if self.parameters.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, p) in self.parameters.iter().enumerate() {
            if i > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            p.emit(w, cx)?;
            cx.token(w, ",")?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<AttributeDeclaration> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
