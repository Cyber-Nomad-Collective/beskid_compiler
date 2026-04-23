use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{Attribute, Identifier, Parameter, Spanned};
use std::fmt::Write;

pub(super) fn emit_generics_list<W: Write>(
    ids: &[Spanned<Identifier>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    if ids.is_empty() {
        return Ok(());
    }
    w.write_char('<')?;
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            cx.token(w, ", ")?;
        }
        id.emit(w, cx)?;
    }
    w.write_char('>')?;
    Ok(())
}

pub(super) fn emit_parameter_list<W: Write>(
    params: &[Spanned<Parameter>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    w.write_char('(')?;
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            cx.token(w, ", ")?;
        }
        p.emit(w, cx)?;
    }
    w.write_char(')')?;
    Ok(())
}

pub(super) fn emit_attribute_lines<W: Write>(
    attrs: &[Spanned<Attribute>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    for (i, a) in attrs.iter().enumerate() {
        if i > 0 {
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        a.emit(w, cx)?;
    }
    Ok(())
}
