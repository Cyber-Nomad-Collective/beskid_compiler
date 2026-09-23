//! Emit Beskid combinator parsers from a constrained Pest grammar surface.

mod ast;
mod emit;
mod emit_assignment;
mod emit_node;
mod expr_parser;
mod rule_name;
mod strings;

#[cfg(test)]
mod tests;

pub use ast::{GrammarRule, parse_grammar_rules};
pub use emit::emit_combinator_module;
pub use rule_name::rule_name_to_callable;
