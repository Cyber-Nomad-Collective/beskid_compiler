use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::emit_attribute_lines;
use crate::syntax::{Spanned, TestDefinition, TestMetaSection, TestMetadataEntry, TestSkipEntry, TestSkipSection};
use std::fmt::Write;

impl Emit for TestMetadataEntry {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        cx.space(w)?;
        w.write_char('=')?;
        cx.space(w)?;
        self.value.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<TestMetadataEntry> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for TestSkipEntry {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        cx.space(w)?;
        w.write_char('=')?;
        cx.space(w)?;
        self.value.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<TestSkipEntry> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for TestMetaSection {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "meta")?;
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

impl Emit for Spanned<TestMetaSection> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for TestSkipSection {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "skip")?;
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

impl Emit for Spanned<TestSkipSection> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for TestDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if !self.attributes.is_empty() {
            emit_attribute_lines(&self.attributes, w, cx)?;
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        self.visibility.emit(w, cx)?;
        cx.token(w, "test")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        if self.meta.is_none() && self.skip.is_none() && self.statements.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }

        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;

        let mut first = true;
        if let Some(meta) = &self.meta {
            cx.write_indent(w)?;
            meta.emit(w, cx)?;
            cx.nl(w)?;
            first = false;
        }
        if let Some(skip) = &self.skip {
            if !first {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            skip.emit(w, cx)?;
            cx.nl(w)?;
            first = false;
        }
        for statement in &self.statements {
            if !first {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            statement.emit(w, cx)?;
            cx.nl(w)?;
            first = false;
        }

        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<TestDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
