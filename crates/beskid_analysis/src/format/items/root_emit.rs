use crate::doc::LeadingDocComment;
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::emit_attribute_lines;
use crate::syntax::{
    InlineModule, Node, Program, Spanned,
};
use std::fmt::Write;

fn emit_leading_doc_lines<W: Write>(
    doc: Option<&LeadingDocComment>,
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    let Some(d) = doc else {
        return Ok(());
    };
    for line in d.normalized_source.lines() {
        cx.write_indent(w)?;
        w.write_str("///")?;
        if !line.is_empty() {
            w.write_char(' ')?;
            w.write_str(line)?;
        }
        cx.nl(w)?;
    }
    Ok(())
}

impl Emit for Program {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                cx.between_top_level_declarations(w)?;
            }
            let doc = self.leading_docs.get(i).and_then(|x| x.as_ref());
            emit_leading_doc_lines(doc, w, cx)?;
            cx.write_indent(w)?;
            item.emit(w, cx)?;
            cx.nl(w)?;
        }
        Ok(())
    }
}

impl Emit for Node {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Node::Function(f) => f.emit(w, cx),
            Node::Method(m) => m.emit(w, cx),
            Node::TypeDefinition(t) => t.emit(w, cx),
            Node::EnumDefinition(e) => e.emit(w, cx),
            Node::ContractDefinition(c) => c.emit(w, cx),
            Node::TestDefinition(t) => t.emit(w, cx),
            Node::AttributeDeclaration(a) => a.emit(w, cx),
            Node::ModuleDeclaration(m) => m.emit(w, cx),
            Node::InlineModule(m) => m.emit(w, cx),
            Node::UseDeclaration(u) => u.emit(w, cx),
        }
    }
}

impl Emit for Spanned<Node> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for InlineModule {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if !self.attributes.is_empty() {
            emit_attribute_lines(&self.attributes, w, cx)?;
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        self.visibility.emit(w, cx)?;
        cx.token(w, "mod")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        if self.items.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                cx.between_top_level_declarations(w)?;
            }
            let doc = self.leading_docs.get(i).and_then(|x| x.as_ref());
            emit_leading_doc_lines(doc, w, cx)?;
            cx.write_indent(w)?;
            item.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<InlineModule> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
