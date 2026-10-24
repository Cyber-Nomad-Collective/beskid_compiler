//! The Beskid formatter: one syntax-only function, [`format_source`], used by `beskid fmt` and
//! the language server. House style lives in `policy.rs`; comments are re-emitted from the
//! source through the line hooks in `emit.rs` and `lines.rs`.

mod attribute_lists;
mod emit;
mod expressions_emit;
mod items;
mod lines;
mod naming_normalize;
mod policy;
mod source;
mod statements_emit;
mod tokens;
mod trivia;
mod types_emit;

#[cfg(test)]
mod tests;

pub use emit::{Emit, EmitCtx, EmitError, Emitter};
pub use source::{FormatError, format_generated_program, format_source};

#[cfg(test)]
mod preservation_tests {
    use super::format_source;
    use crate::services::parse_program;
    use crate::syntax::Node;

    #[test]
    fn inline_type_methods_survive_formatting_with_and_without_fields() {
        for source in [
            "pub type Binding { pub i32 Read() { return 42; } }",
            "pub type Binding { pub i32 count; pub i32 Read() { return this.count; } }",
        ] {
            let original = parse_program(source).expect("parse inline method fixture");
            let Node::TypeDefinition(before) = &original.node.items[0].node else { panic!("type expected") };
            assert_eq!(before.node.methods.len(), 1);
            let formatted = format_source("<memory>", source).unwrap();
            let reparsed = parse_program(&formatted).unwrap();
            let Node::TypeDefinition(after) = &reparsed.node.items[0].node else { panic!("type expected") };
            assert_eq!(after.node.methods.len(), 1, "formatter deleted executable method: {formatted}");
            assert_eq!(after.node.fields.len(), before.node.fields.len());
            assert!(formatted.contains("return"), "method body deleted");
            assert_eq!(format_source("<memory>", &formatted).unwrap(), formatted);
        }
    }
}
