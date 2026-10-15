//! Opinionated pretty-printer (`Emit` trait), mirroring bsharp layout rules.

mod emit;
mod expressions_emit;
mod items;
mod naming_normalize;
mod policy;
mod statements_emit;
mod types_emit;

pub use emit::{Emit, EmitCtx, EmitError, Emitter, emit_error_semantic_diagnostic, format_program};

#[cfg(test)]
mod preservation_tests {
    use super::format_program;
    use crate::services::parse_program;
    use crate::syntax::Node;

    #[test]
    fn inline_type_methods_survive_formatting_with_and_without_fields() {
        for source in [
            "pub type Binding { pub i32 Read() { return 42; } }",
            "pub type Binding { pub i32 count, pub i32 Read() { return this.count; } }",
        ] {
            let original = parse_program(source).expect("parse inline method fixture");
            let Node::TypeDefinition(before) = &original.node.items[0].node else { panic!("type expected") };
            assert_eq!(before.node.methods.len(), 1);
            let formatted = format_program(&original).unwrap();
            let reparsed = parse_program(&formatted).unwrap();
            let Node::TypeDefinition(after) = &reparsed.node.items[0].node else { panic!("type expected") };
            assert_eq!(after.node.methods.len(), 1, "formatter deleted executable method: {formatted}");
            assert_eq!(after.node.fields.len(), before.node.fields.len());
            assert!(formatted.contains("return"), "method body deleted");
            assert_eq!(format_program(&reparsed).unwrap(), formatted);
        }
    }
}
