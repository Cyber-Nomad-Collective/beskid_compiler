use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::{emit_generics_list, emit_parameter_list};
use crate::syntax::{FunctionDefinition, MethodDefinition, Spanned};
use std::fmt::Write;

impl Emit for FunctionDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_generics_list(&self.generics, w, cx)?;
        emit_parameter_list(&self.parameters, w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<FunctionDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for MethodDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_parameter_list(&self.parameters, w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<MethodDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
