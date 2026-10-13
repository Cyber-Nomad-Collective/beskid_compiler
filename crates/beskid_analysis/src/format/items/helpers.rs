use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::attribute_lists::emit_inline_attributes;
use crate::syntax::{Parameter, ReceiverParameter, Spanned, TypeParameter};
use std::fmt::Write;

pub(super) fn emit_generics_list<W: Write>(
    ids: &[Spanned<TypeParameter>],
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
        emit_inline_attributes(&id.node.attributes, w, cx)?;
        w.write_str(&id.node.name)?;
    }
    w.write_char('>')?;
    Ok(())
}

pub(super) fn emit_parameter_list<W: Write>(
    params: &[Spanned<Parameter>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    emit_method_parameter_list(None, params, w, cx)
}

/// A method parameter list whose first entry may be an explicit receiver.
pub(super) fn emit_method_parameter_list<W: Write>(
    receiver: Option<&Spanned<ReceiverParameter>>,
    params: &[Spanned<Parameter>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    w.write_char('(')?;
    if let Some(receiver) = receiver {
        emit_inline_attributes(&receiver.node.attributes, w, cx)?;
        if receiver.node.mutable {
            cx.token(w, "mut")?;
            cx.space(w)?;
        }
        cx.token(w, "this")?;
        if !params.is_empty() {
            cx.token(w, ", ")?;
        }
    }
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            cx.token(w, ", ")?;
        }
        p.emit(w, cx)?;
    }
    w.write_char(')')?;
    Ok(())
}

