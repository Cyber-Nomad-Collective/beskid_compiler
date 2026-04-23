use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::{emit_attribute_lines, emit_generics_list, emit_parameter_list};
use crate::syntax::items::impl_block::ImplBlock;
use crate::syntax::{
    ContractDefinition, ContractEmbedding, ContractMethodSignature, ContractNode, EnumDefinition,
    EnumVariant, ModuleDeclaration, TypeDefinition, UseDeclaration, Spanned,
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
        if !self.attributes.is_empty() {
            emit_attribute_lines(&self.attributes, w, cx)?;
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
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

impl Emit for TypeDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        cx.token(w, "type")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        emit_generics_list(&self.generics, w, cx)?;
        if !self.conformances.is_empty() {
            cx.space(w)?;
            w.write_char(':')?;
            cx.space(w)?;
            for (i, c) in self.conformances.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                c.emit(w, cx)?;
            }
        }
        if self.fields.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, f) in self.fields.iter().enumerate() {
            if i > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            f.emit(w, cx)?;
            cx.token(w, ",")?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<TypeDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for EnumVariant {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        if self.fields.is_empty() {
            return Ok(());
        }
        w.write_char('(')?;
        for (i, f) in self.fields.iter().enumerate() {
            if i > 0 {
                cx.token(w, ", ")?;
            }
            f.emit(w, cx)?;
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

impl Emit for EnumDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        cx.token(w, "enum")?;
        cx.space(w)?;
        self.name.emit(w, cx)?;
        emit_generics_list(&self.generics, w, cx)?;
        if self.variants.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, v) in self.variants.iter().enumerate() {
            if i > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            v.emit(w, cx)?;
            cx.token(w, ",")?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<EnumDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractMethodSignature {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_parameter_list(&self.parameters, w, cx)?;
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
        self.name.emit(w, cx)?;
        w.write_char(';')?;
        Ok(())
    }
}

impl Emit for Spanned<ContractEmbedding> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractNode {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            ContractNode::MethodSignature(m) => m.emit(w, cx),
            ContractNode::Embedding(e) => e.emit(w, cx),
        }
    }
}

impl Emit for Spanned<ContractNode> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ContractDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if !self.attributes.is_empty() {
            emit_attribute_lines(&self.attributes, w, cx)?;
            cx.nl(w)?;
            cx.write_indent(w)?;
        }
        self.visibility.emit(w, cx)?;
        cx.token(w, "contract")?;
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
        for (i, it) in self.items.iter().enumerate() {
            if i > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            it.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<ContractDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for ImplBlock {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        cx.token(w, "impl")?;
        cx.space(w)?;
        self.receiver_type.emit(w, cx)?;
        if self.methods.is_empty() {
            cx.space(w)?;
            w.write_str("{ }")?;
            return Ok(());
        }
        cx.nl(w)?;
        cx.write_indent(w)?;
        cx.open_brace(w)?;
        for (i, method) in self.methods.iter().enumerate() {
            if i > 0 {
                cx.between_members(w)?;
            }
            cx.write_indent(w)?;
            method.emit(w, cx)?;
            cx.nl(w)?;
        }
        cx.close_brace(w)?;
        Ok(())
    }
}

impl Emit for Spanned<ImplBlock> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
