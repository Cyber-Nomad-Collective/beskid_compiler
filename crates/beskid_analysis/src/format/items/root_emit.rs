use crate::doc::LeadingDocComment;
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::emit_attribute_lines;
use crate::syntax::{
    ConstantDefinition, HostBodyItem, HostDefinition, InlineModule, Literal, Node, Program, RegistryBlock, RegistryEntry, ScopeDefinition,
    ScopeHook, ScopeHookKind, Spanned,
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
            Node::HostDefinition(h) => h.emit(w, cx),
            Node::Function(f) => f.emit(w, cx),
            Node::ConstantDefinition(c) => c.emit(w, cx),
            Node::Method(m) => m.emit(w, cx),
            Node::ExtendTypeDefinition(e) => e.emit(w, cx),
            Node::TypeDefinition(t) => t.emit(w, cx),
            Node::EnumDefinition(e) => e.emit(w, cx),
            Node::ContractDefinition(c) => c.emit(w, cx),
            Node::TestDefinition(t) => t.emit(w, cx),
            Node::AttributeDeclaration(a) => a.emit(w, cx),
            Node::ModuleDeclaration(m) => m.emit(w, cx),
            Node::InlineModule(m) => m.emit(w, cx),
            Node::UseDeclaration(u) => u.emit(w, cx),
            Node::MacroDefinition(m) => m.emit(w, cx),
        }
    }
}

impl Emit for ConstantDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "const")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        cx.space(w)?;
        cx.token(w, "=")?;
        cx.space(w)?;
        match &self.value.node {
            Literal::Integer(value) => cx.token(w, value)?,
            _ => unreachable!("constant grammar accepts integer literals only"),
        }
        cx.token(w, ";")
    }
}

impl Emit for Spanned<ConstantDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for HostDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "host")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        w.write_char('(')?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            param.emit(w, cx)?;
        }
        w.write_char(')')?;
        if let Some(base) = &self.base_host {
            cx.space(w)?;
            cx.token(w, ":")?;
            cx.space(w)?;
            base.emit(w, cx)?;
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for item in &self.body {
            cx.write_indent(w)?;
            item.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<HostDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for HostBodyItem {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            HostBodyItem::Registry(r) => r.emit(w, cx),
            HostBodyItem::Registration(entry) => entry.emit(w, cx),
            HostBodyItem::Scope(s) => s.emit(w, cx),
            HostBodyItem::Hook(h) => h.emit(w, cx),
        }
    }
}

impl Emit for Spanned<HostBodyItem> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for RegistryBlock {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "registry")?;
        cx.space(w)?;
        cx.open_brace(w)?;
        for entry in &self.entries {
            cx.write_indent(w)?;
            entry.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<RegistryBlock> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for RegistryEntry {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if let Some(lifetime) = self.lifetime {
            match lifetime {
                crate::syntax::RegistrationLifetime::Single => cx.token(w, "single")?,
                crate::syntax::RegistrationLifetime::Transient => cx.token(w, "transient")?,
            }
            cx.space(w)?;
        }
        self.implementation.emit(w, cx)?;
        if let Some(target) = &self.target {
            cx.space(w)?;
            cx.token(w, "for")?;
            cx.space(w)?;
            target.emit(w, cx)?;
        }
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<RegistryEntry> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ScopeDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "scope")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        w.write_char('(')?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            param.emit(w, cx)?;
        }
        w.write_char(')')?;
        cx.space(w)?;
        cx.open_brace(w)?;
        for item in &self.body {
            cx.write_indent(w)?;
            item.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<ScopeDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ScopeHook {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let name = match self.kind {
            ScopeHookKind::Init => "init",
            ScopeHookKind::Dispose => "dispose",
            ScopeHookKind::Startup => "startup",
        };
        cx.token(w, name)?;
        w.write_char('(')?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            param.emit(w, cx)?;
        }
        w.write_char(')')?;
        cx.nl(w)?;
        cx.write_indent(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<ScopeHook> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
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
