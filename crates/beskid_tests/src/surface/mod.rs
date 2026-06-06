//! Surface syntax: pest rules plus AST shape (merged parsing + syntax layers).

pub mod ast;
pub mod util;

#[cfg(test)]
mod attributes;
#[cfg(test)]
mod contracts;
#[cfg(test)]
mod control_flow;
#[cfg(test)]
mod enums;
#[cfg(test)]
mod expression_rules;
#[cfg(test)]
mod expressions;
#[cfg(test)]
mod items;
#[cfg(test)]
mod item_rules;
#[cfg(test)]
mod lexical;
#[cfg(test)]
mod literals;
#[cfg(test)]
mod match_expr;
#[cfg(test)]
mod methods;
#[cfg(test)]
mod modules;
#[cfg(test)]
mod patterns;
#[cfg(test)]
mod query;
#[cfg(test)]
mod statements;
#[cfg(test)]
mod string_rules;
#[cfg(test)]
mod struct_literals;
#[cfg(test)]
mod types;
