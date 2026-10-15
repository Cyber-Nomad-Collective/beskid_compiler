//! Attribute list placement shared by every declaration site.
//!
//! Inline sites (parameters, receivers, lambdas, lambda parameters, type parameters) keep the
//! list on the same line, followed by one space. Every other site puts each attribute on its
//! own line before the declaration.

use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{Attribute, Spanned};
use std::fmt::Write;

pub(crate) fn emit_inline_attributes<W: Write>(
    attributes: &[Spanned<Attribute>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    for attribute in attributes {
        attribute.emit(w, cx)?;
        cx.space(w)?;
    }
    Ok(())
}

pub(crate) fn emit_attribute_block<W: Write>(
    attributes: &[Spanned<Attribute>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    for attribute in attributes {
        attribute.emit(w, cx)?;
        cx.nl(w)?;
        cx.write_indent(w)?;
    }
    Ok(())
}
