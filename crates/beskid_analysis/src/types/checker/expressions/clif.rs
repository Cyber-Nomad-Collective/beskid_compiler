use crate::clif_surface::{ClifBlockMode, ClifBlockSurface, CLIF_PAYLOAD_ELEMENT_TYPES, parse_clif_surface_in};
use crate::syntax::{ClifBlockExpression, PrimitiveType, Spanned};
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    /// A CLIF block has no intrinsic type: it takes the type its context expects (a return
    /// value, a typed `let`, or a call argument). The block's own instructions are checked by
    /// lowering; here we validate the surface and every `%N` against the declared parameters.
    pub(in crate::types::checker) fn type_clif_block(&mut self, clif: &Spanned<ClifBlockExpression>) -> Option<TypeId> {
        let span = clif.span;
        let Some(expected) = self.contextual_expected_type else {
            self.errors.push(TypeError::InvalidClifBlock {
                span,
                detail: "a clif block needs a typed context: use it as a return value, a typed `let` initializer, \
                         or a call argument"
                    .to_owned(),
            });
            return None;
        };
        if !self.is_clif_scalar(expected) {
            self.errors.push(TypeError::InvalidClifBlock {
                span,
                detail: "a clif block must produce a scalar value (bool, char, integer, f64, word, or pointer)"
                    .to_owned(),
            });
            return Some(expected);
        }
        self.check_clif_block_body(clif, ClifBlockMode::Value);
        Some(expected)
    }

    /// A CLIF block used as an expression statement yields no value; it may end with any
    /// statement (for example a `store` or a call with no result).
    pub(in crate::types::checker) fn type_clif_block_statement(&mut self, clif: &Spanned<ClifBlockExpression>) {
        self.check_clif_block_body(clif, ClifBlockMode::Statement);
    }

    fn check_clif_block_body(&mut self, clif: &Spanned<ClifBlockExpression>, mode: ClifBlockMode) {
        let span = clif.span;
        let surface = match parse_clif_surface_in(&clif.node.body, mode) {
            Ok(surface) => surface,
            Err(error) => {
                self.errors.push(TypeError::InvalidClifBlock { span, detail: error.to_string() });
                return;
            }
        };
        self.check_clif_parameters(span, &surface);
    }

    fn check_clif_parameters(&mut self, span: crate::syntax::SpanInfo, surface: &ClifBlockSurface) {
        let Some(parameters) = self.current_clif_parameters.clone() else {
            if !surface.referenced_parameters().is_empty() {
                self.errors.push(TypeError::InvalidClifBlock {
                    span,
                    detail: "clif blocks inside lambdas cannot name parameters with `%N`".to_owned(),
                });
            }
            return;
        };
        for index in surface.referenced_parameters() {
            if index >= parameters.len() {
                self.errors.push(TypeError::InvalidClifBlock {
                    span,
                    detail: format!(
                        "`%{index}` does not name a parameter; the enclosing function has {} CLIF parameter(s)",
                        parameters.len()
                    ),
                });
            }
        }
        for index in surface.value_parameters() {
            let Some(Some(type_id)) = parameters.get(index) else {
                if parameters.get(index).is_some() {
                    self.errors.push(TypeError::InvalidClifBlock {
                        span,
                        detail: format!("`%{index}` is a method receiver or has an unresolved type"),
                    });
                }
                continue;
            };
            if !self.is_clif_scalar(*type_id) {
                self.errors.push(TypeError::InvalidClifBlock {
                    span,
                    detail: format!(
                        "`%{index}` is not a scalar parameter; read arrays through `payload %{index}` and `length \
                         %{index}`"
                    ),
                });
            }
        }
        for index in surface.array_parameters() {
            let Some(Some(type_id)) = parameters.get(index) else {
                if parameters.get(index).is_some() {
                    self.errors.push(TypeError::InvalidClifBlock {
                        span,
                        detail: format!("`%{index}` is a method receiver or has an unresolved type"),
                    });
                }
                continue;
            };
            if self.clif_payload_element(*type_id).is_none() {
                let allowed =
                    CLIF_PAYLOAD_ELEMENT_TYPES.iter().map(|(name, _)| format!("{name}[]")).collect::<Vec<_>>();
                self.errors.push(TypeError::InvalidClifBlock {
                    span,
                    detail: format!("`payload`/`length` require a parameter of type {}", allowed.join(", ")),
                });
            }
        }
    }

    fn is_clif_scalar(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(
                PrimitiveType::Bool
                    | PrimitiveType::Char
                    | PrimitiveType::I32
                    | PrimitiveType::I64
                    | PrimitiveType::U32
                    | PrimitiveType::U8
                    | PrimitiveType::F64
                    | PrimitiveType::Word
                    | PrimitiveType::Pointer
            ))
        )
    }

    fn clif_payload_element(&self, type_id: TypeId) -> Option<u8> {
        let Some(TypeInfo::Array(element)) = self.type_table.get(type_id) else {
            return None;
        };
        match self.type_table.get(*element) {
            Some(TypeInfo::Primitive(PrimitiveType::U8)) => Some(1),
            Some(TypeInfo::Primitive(PrimitiveType::U32)) => Some(4),
            Some(TypeInfo::Primitive(PrimitiveType::I64)) => Some(8),
            _ => None,
        }
    }
}
