use crate::doc::LeadingDocComment;
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::lines::{Line, body_is_empty, emit_line, open_body};
use crate::format::policy::{ItemKind, blank_between_items, item_kind};
use crate::syntax::{
    ConstantDefinition, HostBodyItem, HostDefinition, InlineModule, Literal, Node, Program, RegistryBlock,
    RegistryEntry, ScopeDefinition, ScopeHook, ScopeHookKind, Spanned,
};
use std::fmt::Write;

/// Emit top-level or inline-module items with their documentation, comments and separation.
fn emit_items<W: Write>(
    items: &[Spanned<Node>],
    docs: &[Option<LeadingDocComment>],
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    let mut previous: Option<ItemKind> = None;
    for (i, item) in items.iter().enumerate() {
        let kind = item_kind(&item.node);
        let line = Line::new(item.span)
            .doc(docs.get(i).and_then(Option::as_ref))
            .blank(blank_between_items(previous, kind));
        emit_line(w, cx, line, "", |w, cx| item.emit(w, cx))?;
        previous = Some(kind);
    }
    Ok(())
}

impl Emit for Program {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        emit_items(&self.items, &self.leading_docs, w, cx)?;
        cx.finish(w)
    }
}

impl Emit for Node {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Node::HostDefinition(h) => h.emit(w, cx),
            Node::Function(f) => f.emit(w, cx),
            Node::ConstantDefinition(c) => c.emit(w, cx),
            Node::Method(m) => m.emit(w, cx),
            Node::ImplBlock(i) => i.emit(w, cx),
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
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
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

impl Emit for Spanned<HostDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let container_end = self.span.end;
        let this = &self.node;
        cx.token(w, "host")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        w.write_char('(')?;
        for (i, param) in this.parameters.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            param.emit(w, cx)?;
        }
        w.write_char(')')?;
        if let Some(base) = &this.base_host {
            cx.space(w)?;
            cx.token(w, ":")?;
            cx.space(w)?;
            base.emit(w, cx)?;
        }
        if body_is_empty(cx, this.body.len(), container_end) {
            cx.space(w)?;
            return cx.token(w, "{ }");
        }
        open_body(w, cx)?;
        for item in &this.body {
            emit_line(w, cx, Line::new(item.span), "", |w, cx| item.emit(w, cx))?;
        }
        cx.close_brace(w, container_end)?;
        Ok(())
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

impl Emit for Spanned<RegistryBlock> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "registry")?;
        if body_is_empty(cx, self.node.entries.len(), self.span.end) {
            cx.space(w)?;
            return cx.token(w, "{ }");
        }
        open_body(w, cx)?;
        for entry in &self.node.entries {
            emit_line(w, cx, Line::new(entry.span), "", |w, cx| entry.emit(w, cx))?;
        }
        cx.close_brace(w, self.span.end)
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

impl Emit for Spanned<ScopeDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let container_end = self.span.end;
        let this = &self.node;
        cx.token(w, "scope")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        w.write_char('(')?;
        for (i, param) in this.parameters.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            param.emit(w, cx)?;
        }
        w.write_char(')')?;
        if body_is_empty(cx, this.body.len(), container_end) {
            cx.space(w)?;
            return cx.token(w, "{ }");
        }
        open_body(w, cx)?;
        for item in &this.body {
            emit_line(w, cx, Line::new(item.span), "", |w, cx| item.emit(w, cx))?;
        }
        cx.close_brace(w, container_end)
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
        cx.before_open_brace(w)?;
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

impl Emit for Spanned<InlineModule> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        this.visibility.emit(w, cx)?;
        cx.token(w, "mod")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        if body_is_empty(cx, this.items.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        emit_items(&this.items, &this.leading_docs, w, cx)?;
        cx.close_brace(w, self.span.end)
    }
}
