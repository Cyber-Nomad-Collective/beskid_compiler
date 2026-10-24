use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::doc::LeadingDocComment;
use crate::format::attribute_lists::emit_inline_attributes;
use crate::format::lines::emit_leading_doc_lines;
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
    docs: &[Option<LeadingDocComment>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    emit_method_parameter_list(None, params, docs, w, cx)
}

/// A method parameter list whose first entry may be an explicit receiver.
///
/// A list with documented parameters goes one parameter per line so each keeps its `///` lines.
pub(super) fn emit_method_parameter_list<W: Write>(
    receiver: Option<&Spanned<ReceiverParameter>>,
    params: &[Spanned<Parameter>],
    docs: &[Option<LeadingDocComment>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    let multiline = docs.iter().any(Option::is_some);
    w.write_char('(')?;
    if multiline {
        cx.push_indent();
    }
    let mut first = true;
    if let Some(receiver) = receiver {
        if multiline {
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        emit_inline_attributes(&receiver.node.attributes, w, cx)?;
        if receiver.node.mutable {
            cx.token(w, "mut")?;
            cx.space(w)?;
        }
        if receiver.node.movable {
            cx.token(w, "move")?;
            cx.space(w)?;
        }
        cx.token(w, "this")?;
        first = false;
    }
    for (i, p) in params.iter().enumerate() {
        if !first {
            w.write_char(',')?;
            if !multiline {
                cx.space(w)?;
            }
        }
        first = false;
        if multiline {
            cx.nl(w)?;
            emit_leading_doc_lines(docs.get(i).and_then(Option::as_ref), w, cx)?;
            cx.write_indent(w)?;
        }
        p.emit(w, cx)?;
    }
    if multiline {
        cx.pop_indent();
    }
    w.write_char(')')?;
    Ok(())
}
