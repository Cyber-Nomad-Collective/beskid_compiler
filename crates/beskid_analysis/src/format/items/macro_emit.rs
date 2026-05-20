use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{
    MacroDefinition, MacroInvocation, MacroMetavariable, Spanned,
};
use std::fmt::Write;

impl Emit for MacroDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        write!(w, "macro ")?;
        self.name.emit(w, cx)?;
        write!(w, " ")?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<MacroDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MacroInvocation {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        write!(w, "!(").map_err(EmitError)?;
        Ok(())
    }
}

impl Emit for Spanned<MacroInvocation> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MacroMetavariable {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        write!(w, "${}", self.name.node.name).map_err(EmitError)
    }
}

impl Emit for Spanned<MacroMetavariable> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
