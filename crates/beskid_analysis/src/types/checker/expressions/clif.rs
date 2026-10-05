use crate::clif_surface::{CLIF_PAYLOAD_ELEMENT_TYPES, parse_clif_surface};
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
        let surface = match parse_clif_surface(&clif.node.body) {
            Ok(surface) => surface,
            Err(error) => {
                self.errors.push(TypeError::InvalidClifBlock { span, detail: error.to_string() });
                return Some(expected);
            }
        };
        let Some(parameters) = self.current_clif_parameters.clone() else {
            if !surface.referenced_parameters().is_empty() {
                self.errors.push(TypeError::InvalidClifBlock {
                    span,
                    detail: "clif blocks inside lambdas cannot name parameters with `%N`".to_owned(),
                });
            }
            return Some(expected);
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
        Some(expected)
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

#[cfg(test)]
mod tests {
    /// Resolve and type-check `source`; return every recorded error as debug text (empty on success).
    fn type_errors(source: &str) -> String {
        let program = crate::services::parse_program_with_source_name("ClifBlock.bd", source).expect("source parses");
        match crate::services::resolve_and_type_program(&program) {
            Ok(_) => String::new(),
            Err(error) => format!("{error:?}"),
        }
    }

    fn clif_diagnostics(source: &str) -> Vec<String> {
        let errors = type_errors(source);
        if errors.is_empty() { Vec::new() } else { vec![errors] }
    }

    #[test]
    fn clif_block_takes_return_let_and_argument_types() {
        let source = r#"
pub i64 Echo(i64 value) {
    return clif { return %0 };
}

pub i64 Twice(i64 value) {
    i64 doubled = clif {
        %d = iadd %0, %0
        return %d
    };
    return doubled;
}

pub f64 Root(f64 value) {
    return clif { call @sqrt(%0) };
}

pub u32 Mask(u32[] words, u32 mask) {
    return clif {
        %p = payload %0
        %w = load.i32 %p
        %m = band %w, %1
        return %m
    };
}

pub i64 Use(i64 value) {
    return Echo(clif { return %0 });
}
"#;
        assert_eq!(clif_diagnostics(source), Vec::<String>::new());
    }

    #[test]
    fn clif_block_without_typed_context_is_rejected() {
        let diagnostics = clif_diagnostics("pub i64 Main() {\n    clif { return %0 };\n    return 0;\n}\n");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.contains("needs a typed context")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn clif_block_validates_parameters_and_surface() {
        let source = r#"
pub i64 OutOfRange(i64 value) {
    return clif { return %3 };
}

pub i64 NotAnArray(string text) {
    return clif {
        %p = payload %0
        %v = load.i64 %p
        return %v
    };
}

pub i64 ArrayAsValue(i64[] values) {
    return clif {
        %v = iadd %0, %0
        return %v
    };
}

pub i64 Branches(i64 value) {
    return clif {
        %v = jump block1
        return %v
    };
}
"#;
        let diagnostics = clif_diagnostics(source);
        for expected in [
            "`%3` does not name a parameter",
            "require a parameter of type u8[], u32[], i64[]",
            "`%0` is not a scalar parameter",
            "opcode `jump` is not allowed",
        ] {
            assert!(diagnostics.iter().any(|diagnostic| diagnostic.contains(expected)), "{expected}: {diagnostics:?}");
        }
    }

    #[test]
    fn clif_block_inside_lambda_is_rejected() {
        let source = r#"
pub i64 Outer(i64 value) {
    (i64) => i64 f = (i64 x) => clif { return %0 };
    return f(value);
}
"#;
        let diagnostics = clif_diagnostics(source);
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.contains("InvalidClifBlock")), "{diagnostics:?}");
    }
}
