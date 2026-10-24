use crate::doc::LeadingDocComment;
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::{emit_generics_list, emit_parameter_list};
use crate::format::lines::{Line, body_is_empty, emit_leading_doc_lines, emit_line, open_body};
use crate::syntax::items::impl_block::ImplBlock;
use crate::syntax::{
    AssociatedTypeBinding, ContractAssociatedType, ContractDefinition, ContractEmbedding, ContractMethodSignature,
    ContractNode, EnumDefinition, EnumVariant, ExtendTypeDefinition, Field, MethodDefinition, ModuleDeclaration,
    SpanInfo, Spanned, TypeDefinition, UseDeclaration,
};
use std::fmt::Write;

impl Emit for UseDeclaration {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        cx.token(w, "use")?;
        cx.space(w)?;
        self.path.emit(w, cx)?;
        if let Some(alias) = &self.alias {
            cx.space(w)?;
            cx.token(w, "as")?;
            cx.space(w)?;
            alias.emit(w, cx)?;
        }
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<UseDeclaration> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ModuleDeclaration {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
        self.visibility.emit(w, cx)?;
        cx.token(w, "mod")?;
        cx.space(w)?;
        self.path.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ModuleDeclaration> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

/// One member of a type or impl body, in source order.
enum Member<'a> {
    Field(&'a Spanned<Field>, Option<&'a LeadingDocComment>),
    Binding(&'a Spanned<AssociatedTypeBinding>),
    Method(&'a Spanned<MethodDefinition>, Option<&'a LeadingDocComment>),
}

impl Member<'_> {
    fn span(&self) -> SpanInfo {
        match self {
            Member::Field(f, _) => f.span,
            Member::Binding(b) => b.span,
            Member::Method(m, _) => m.span,
        }
    }

    /// Fields and associated type bindings are compact kinds; methods always stand apart.
    fn kind(&self) -> u8 {
        match self {
            Member::Field(..) => 0,
            Member::Binding(_) => 1,
            Member::Method(..) => 2,
        }
    }
}

/// Emit type or impl members in source order: compact runs of one kind, a blank line between
/// kinds and around every method.
fn emit_members<W: Write>(mut members: Vec<Member<'_>>, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
    members.sort_by_key(|m| m.span().start);
    let mut previous: Option<u8> = None;
    for member in &members {
        let kind = member.kind();
        let blank = previous.is_some_and(|p| p != kind || kind == 2);
        previous = Some(kind);
        match member {
            Member::Field(field, doc) => {
                emit_line(w, cx, Line::new(field.span).doc(*doc).blank(blank), ";", |w, cx| field.emit(w, cx))?
            }
            Member::Binding(binding) => emit_line(w, cx, Line::new(binding.span).blank(blank), ";", |w, cx| {
                cx.token(w, "type ")?;
                binding.node.name.emit(w, cx)?;
                cx.token(w, " = ")?;
                binding.node.ty.emit(w, cx)
            })?,
            Member::Method(method, doc) => {
                emit_line(w, cx, Line::new(method.span).doc(*doc).blank(blank), "", |w, cx| method.emit(w, cx))?
            }
        }
    }
    Ok(())
}

fn methods_of<'a>(
    methods: &'a [Spanned<MethodDefinition>],
    docs: &'a [Option<LeadingDocComment>],
) -> impl Iterator<Item = Member<'a>> {
    methods.iter().enumerate().map(|(i, m)| Member::Method(m, docs.get(i).and_then(Option::as_ref)))
}

impl Emit for Spanned<TypeDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        this.visibility.emit(w, cx)?;
        cx.token(w, "type")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        emit_generics_list(&this.generics, w, cx)?;
        if !this.conformances.is_empty() {
            cx.space(w)?;
            w.write_char(':')?;
            cx.space(w)?;
            for (i, c) in this.conformances.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                c.emit(w, cx)?;
            }
        }
        let count = this.fields.len() + this.methods.len() + this.associated_type_bindings.len();
        if body_is_empty(cx, count, self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        let mut members: Vec<Member<'_>> = this
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| Member::Field(f, this.field_docs.get(i).and_then(Option::as_ref)))
            .collect();
        members.extend(this.associated_type_bindings.iter().map(Member::Binding));
        members.extend(methods_of(&this.methods, &this.method_docs));
        emit_members(members, w, cx)?;
        cx.close_brace(w, self.span.end)
    }
}

impl Emit for EnumVariant {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
        self.name.emit(w, cx)?;
        if self.fields.is_empty() {
            return Ok(());
        }
        w.write_char('(')?;
        if self.field_docs.iter().any(Option::is_some) {
            // Documented fields go one per line so each keeps its `///` lines.
            cx.push_indent();
            for (i, f) in self.fields.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                cx.nl(w)?;
                emit_leading_doc_lines(self.field_docs.get(i).and_then(Option::as_ref), w, cx)?;
                cx.write_indent(w)?;
                f.emit(w, cx)?;
            }
            cx.pop_indent();
        } else {
            for (i, f) in self.fields.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                f.emit(w, cx)?;
            }
        }
        w.write_char(')')?;
        Ok(())
    }
}

impl Emit for Spanned<EnumVariant> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Spanned<EnumDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        this.visibility.emit(w, cx)?;
        cx.token(w, "enum")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        emit_generics_list(&this.generics, w, cx)?;
        if body_is_empty(cx, this.variants.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        for (i, v) in this.variants.iter().enumerate() {
            let line = Line::new(v.span).doc(this.variant_docs.get(i).and_then(Option::as_ref));
            emit_line(w, cx, line, ",", |w, cx| v.emit(w, cx))?;
        }
        cx.close_brace(w, self.span.end)
    }
}

impl Emit for ContractMethodSignature {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_parameter_list(&self.parameters, &self.parameter_docs, w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ContractMethodSignature> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractEmbedding {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
        self.name.emit(w, cx)?;
        if !self.type_args.is_empty() {
            w.write_char('<')?;
            for (i, arg) in self.type_args.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                arg.emit(w, cx)?;
            }
            w.write_char('>')?;
        }
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ContractEmbedding> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractAssociatedType {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        crate::format::attribute_lists::emit_attribute_block(&self.attributes, w, cx)?;
        cx.token(w, "type")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        if let Some(default) = &self.default {
            cx.space(w)?;
            w.write_char('=')?;
            cx.space(w)?;
            default.emit(w, cx)?;
        }
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ContractAssociatedType> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractNode {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            ContractNode::MethodSignature(m) => m.emit(w, cx),
            ContractNode::Embedding(e) => e.emit(w, cx),
            ContractNode::AssociatedType(a) => a.emit(w, cx),
        }
    }
}

impl Emit for Spanned<ContractNode> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Spanned<ContractDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        this.visibility.emit(w, cx)?;
        cx.token(w, "contract")?;
        cx.space(w)?;
        this.name.emit(w, cx)?;
        emit_generics_list(&this.generics, w, cx)?;
        if body_is_empty(cx, this.items.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        for (i, it) in this.items.iter().enumerate() {
            let line = Line::new(it.span).doc(this.item_docs.get(i).and_then(Option::as_ref));
            emit_line(w, cx, line, "", |w, cx| it.emit(w, cx))?;
        }
        cx.close_brace(w, self.span.end)
    }
}

impl Emit for Spanned<ImplBlock> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        crate::format::attribute_lists::emit_attribute_block(&this.attributes, w, cx)?;
        cx.token(w, "impl")?;
        emit_generics_list(&this.generics, w, cx)?;
        cx.space(w)?;
        this.receiver_type.emit(w, cx)?;
        if !this.conformances.is_empty() {
            cx.space(w)?;
            w.write_char(':')?;
            cx.space(w)?;
            for (i, c) in this.conformances.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                c.emit(w, cx)?;
            }
        }
        if !this.where_bounds.is_empty() {
            cx.token(w, " where ")?;
            for (i, bound) in this.where_bounds.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                bound.parameter.emit(w, cx)?;
                cx.token(w, ": ")?;
                bound.contract.emit(w, cx)?;
            }
        }
        let count = this.methods.len() + this.associated_type_bindings.len();
        if body_is_empty(cx, count, self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        let mut members: Vec<Member<'_>> = this.associated_type_bindings.iter().map(Member::Binding).collect();
        members.extend(methods_of(&this.methods, &this.method_docs));
        emit_members(members, w, cx)?;
        cx.close_brace(w, self.span.end)
    }
}

impl Emit for Spanned<ExtendTypeDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        let this = &self.node;
        cx.token(w, "extend")?;
        cx.space(w)?;
        cx.token(w, "type")?;
        cx.space(w)?;
        this.target_type.emit(w, cx)?;
        if body_is_empty(cx, this.methods.len(), self.span.end) {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        open_body(w, cx)?;
        emit_members(methods_of(&this.methods, &this.method_docs).collect(), w, cx)?;
        cx.close_brace(w, self.span.end)
    }
}
