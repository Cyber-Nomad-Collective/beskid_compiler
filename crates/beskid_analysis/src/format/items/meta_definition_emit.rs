use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::emit_attribute_lines;
use crate::syntax::{MetaDefinition, Spanned};
use std::fmt::Write;

impl Emit for MetaDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if !self.attributes.is_empty() {
            emit_attribute_lines(&self.attributes, w, cx)?;
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        self.visibility.emit(w, cx)?;
        cx.token(w, "meta")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        if self.entries.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (idx, entry) in self.entries.iter().enumerate() {
            if idx > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            entry.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<MetaDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
