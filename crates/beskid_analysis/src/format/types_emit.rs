use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::syntax::{
    EnumPath, Field, FieldKind, Identifier, Parameter, Path, PathSegment, PrimitiveType, Spanned,
    Type, Visibility,
};
use std::fmt::Write;

impl Emit for Identifier {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_str(&self.name)?;
        Ok(())
    }
}

impl Emit for Spanned<Identifier> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Visibility {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Visibility::Public => {
                cx.token(w, "pub")?;
                cx.space(w)?;
            }
            Visibility::Private => {}
        }
        Ok(())
    }
}

impl Emit for Spanned<Visibility> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for PrimitiveType {
    fn emit<W: Write>(&self, w: &mut W, _cx: &mut EmitCtx) -> Result<(), EmitError> {
        w.write_str(match self {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::U8 => "u8",
            PrimitiveType::Pointer => "pointer",
            PrimitiveType::Word => "word",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Char => "char",
            PrimitiveType::String => "string",
            PrimitiveType::Unit => "unit",
            PrimitiveType::Never => "never",
        })?;
        Ok(())
    }
}

impl Emit for Spanned<PrimitiveType> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for PathSegment {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.name.emit(w, cx)?;
        if !self.type_args.is_empty() {
            w.write_char('<')?;
            for (i, t) in self.type_args.iter().enumerate() {
                if i > 0 {
                    cx.token(w, ", ")?;
                }
                t.emit(w, cx)?;
            }
            w.write_char('>')?;
        }
        Ok(())
    }
}

impl Emit for Spanned<PathSegment> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Path {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                w.write_char('.')?;
            }
            seg.emit(w, cx)?;
        }
        Ok(())
    }
}

impl Emit for Spanned<Path> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Type {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        match self {
            Type::Primitive(p) => p.emit(w, cx),
            Type::Complex(p) => p.emit(w, cx),
            Type::Array(inner) => {
                inner.emit(w, cx)?;
                w.write_str("[]")?;
                Ok(())
            }
            Type::Function {
                return_type,
                parameters,
            } => {
                return_type.emit(w, cx)?;
                w.write_char('(')?;
                for (i, p) in parameters.iter().enumerate() {
                    if i > 0 {
                        cx.token(w, ", ")?;
                    }
                    p.emit(w, cx)?;
                }
                w.write_char(')')?;
                Ok(())
            }
        }
    }
}

impl Emit for Spanned<Type> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Parameter {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if self.mutable {
            cx.token(w, "mut")?;
            cx.space(w)?;
        }
        self.ty.emit(w, cx)?;
        cx.space(w)?;
        self.name.emit(w, cx)
    }
}

impl Emit for Spanned<Parameter> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Field {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.visibility.emit(w, cx)?;
        match self.kind {
            FieldKind::Value => {
                self.ty.emit(w, cx)?;
                cx.space(w)?;
                self.name.emit(w, cx)?;
                Ok(())
            }
            FieldKind::Event => {
                cx.token(w, "event")?;
                if let Some(n) = self.event_capacity {
                    w.write_char('{')?;
                    write!(w, "{n}")?;
                    w.write_char('}')?;
                    cx.space(w)?;
                }
                self.name.emit(w, cx)?;
                w.write_char('(')?;
                if let Type::Function { parameters, .. } = &self.ty.node {
                    for (i, p) in parameters.iter().enumerate() {
                        if i > 0 {
                            cx.token(w, ", ")?;
                        }
                        p.emit(w, cx)?;
                    }
                }
                w.write_char(')')?;
                Ok(())
            }
            FieldKind::Injected => {
                cx.token(w, "inject")?;
                cx.space(w)?;
                if let Some(qualifier) = self.inject_qualifier {
                    match qualifier {
                        crate::syntax::InjectQualifier::Global => cx.token(w, "global::")?,
                        crate::syntax::InjectQualifier::Parent => cx.token(w, "parent::")?,
                    }
                }
                self.ty.emit(w, cx)?;
                cx.space(w)?;
                self.name.emit(w, cx)?;
                Ok(())
            }
        }
    }
}

impl Emit for Spanned<Field> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for EnumPath {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.type_path.emit(w, cx)?;
        cx.token(w, "::")?;
        self.variant.emit(w, cx)
    }
}

impl Emit for Spanned<EnumPath> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
