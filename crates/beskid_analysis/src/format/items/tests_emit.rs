use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::lines::{Line, body_is_empty, emit_line, open_body};
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

impl Emit for Spanned<TestMetaSection> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "meta")?;
        if body_is_empty(cx, self.node.entries.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        for entry in &self.node.entries {
            emit_line(w, cx, Line::new(entry.span), "", |w, cx| entry.emit(w, cx))?;
        }
        cx.close_brace(w, self.span.end)
    }
}

impl Emit for Spanned<TestSkipSection> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "skip")?;
        if body_is_empty(cx, self.node.entries.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        for entry in &self.node.entries {
            emit_line(w, cx, Line::new(entry.span), "", |w, cx| entry.emit(w, cx))?;
        }
        cx.close_brace(w, self.span.end)
    }
}

/// One element of a test body, in source order.
enum TestItem<'a> {
    Meta(&'a Spanned<TestMetaSection>),
    Skip(&'a Spanned<TestSkipSection>),
    Statement(&'a Spanned<crate::syntax::Statement>, Option<&'a crate::doc::LeadingDocComment>),
}

impl Emit for Spanned<TestDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        this.visibility.emit(w, cx)?;
        cx.token(w, "test")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        let count = this.statements.len() + usize::from(this.meta.is_some()) + usize::from(this.skip.is_some());
        if body_is_empty(cx, count, self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;

        let mut items: Vec<(usize, TestItem<'_>)> = Vec::new();
        if let Some(meta) = &this.meta {
            items.push((meta.span.start, TestItem::Meta(meta)));
        }
        if let Some(skip) = &this.skip {
            items.push((skip.span.start, TestItem::Skip(skip)));
        }
        for (i, statement) in this.statements.iter().enumerate() {
            let doc = this.statement_docs.get(i).and_then(Option::as_ref);
            let start = doc.map_or(statement.span.start, |d| d.span.start.min(statement.span.start));
            items.push((start, TestItem::Statement(statement, doc)));
        }
        items.sort_by_key(|(start, _)| *start);

        // `meta` and `skip` sections stand apart from the statements; statements keep the
        // author's grouping.
        let mut previous_was_section: Option<bool> = None;
        for (_, item) in &items {
            let is_section = !matches!(item, TestItem::Statement(..));
            let blank = previous_was_section.is_some_and(|p| p || is_section);
            previous_was_section = Some(is_section);
            match item {
                TestItem::Meta(meta) => emit_line(w, cx, Line::new(meta.span).blank(blank), "", |w, cx| meta.emit(w, cx))?,
                TestItem::Skip(skip) => emit_line(w, cx, Line::new(skip.span).blank(blank), "", |w, cx| skip.emit(w, cx))?,
                TestItem::Statement(statement, doc) => {
                    emit_line(w, cx, Line::new(statement.span).doc(*doc).blank(blank), "", |w, cx| statement.emit(w, cx))?
                }
            }
        }
        cx.close_brace(w, self.span.end)
    }
}
