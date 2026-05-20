use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::items::{MacroFragmentKind, MacroParameter};
use crate::syntax::{
    MacroDefinition, MacroInvocation, MacroMetavariable, Spanned,
};
use std::fmt::Write;

impl Emit for MacroFragmentKind {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_str(crate::macros::fragment_kind_keyword(*self)).map_err(EmitError)
    }
}

impl Emit for Spanned<MacroFragmentKind> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MacroParameter {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.kind.emit(w, cx)?;
        write!(w, " ").map_err(EmitError)?;
        self.name.emit(w, cx)
    }
}

impl Emit for Spanned<MacroParameter> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MacroDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        write!(w, "macro ")?;
        self.name.emit(w, cx)?;
        if !self.parameters.is_empty() {
            write!(w, " (")?;
            for (index, param) in self.parameters.iter().enumerate() {
                if index > 0 {
                    write!(w, ", ")?;
                }
                param.emit(w, cx)?;
            }
            write!(w, ")")?;
        }
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
        if !self.arguments.is_empty() || self.block.is_none() {
            write!(w, "!(")?;
            for (index, arg) in self.arguments.iter().enumerate() {
                if index > 0 {
                    write!(w, ", ")?;
                }
                arg.emit(w, cx)?;
            }
            write!(w, ")")?;
        }
        if let Some(block) = &self.block {
            write!(w, " ")?;
            block.emit(w, cx)?;
        }
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
