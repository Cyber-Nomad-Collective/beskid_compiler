//! The Beskid formatter: one syntax-only function, [`format_source`], used by `beskid fmt` and
//! the language server. House style lives in `policy.rs`; comments are re-emitted from the
//! source through the line hooks in `emit.rs` and `lines.rs`.

mod attribute_lists;
mod emit;
mod expressions_emit;
mod items;
mod lines;
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

    #[test]
    fn observation_formatting_keeps_the_callback_and_receiver() {
        let source = "unit Main(){observe Holder().Counters[0].Count(old,new)=>{Use(old,new);};}";
        let formatted = format_source("<memory>", source).unwrap();
        assert!(formatted.contains("observe Holder().Counters[0].Count (old, new) =>"), "{formatted}");
        assert!(formatted.contains("Use(old, new);"), "{formatted}");
        assert_eq!(format_source("<memory>", &formatted).unwrap(), formatted);
    }

    #[test]
    fn observation_formatting_keeps_always_on_both_stored_forms() {
        let source = "type Counter {pub always prop i64 Count=0; always i64 Limit{get;set;}=0;}";
        let formatted = format_source("<memory>", source).unwrap();
        assert!(formatted.contains("pub always prop i64 Count = 0;"), "{formatted}");
        assert!(formatted.contains("always i64 Limit"), "{formatted}");
        assert_eq!(format_source("<memory>", &formatted).unwrap(), formatted);
    }

    #[test]
    fn contract_property_accessors_round_trip_as_one_source_member() {
        let source = "contract Meter<T>{T Value{set;get;} T ReadOnly{get;} unit Reset();}";
        let formatted = format_source("<memory>", source).unwrap();
        assert_eq!(formatted.matches("T Value { set; get; }").count(), 1, "{formatted}");
        assert!(formatted.contains("T ReadOnly { get; }"), "{formatted}");
        assert!(formatted.contains("unit Reset();"), "{formatted}");
        assert_eq!(format_source("<memory>", &formatted).unwrap(), formatted);
    }
}
