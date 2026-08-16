//! Expression AST: operators, calls, control-flow expressions, literals, and patterns.

pub mod array_literal_expression;
pub mod assign_expression;
pub mod binary_expression;
pub mod block_expression;
pub mod call_expression;
pub mod clif_block;
pub mod code_string;
pub mod enum_constructor_expression;
pub mod expression;
pub mod grouped_expression;
pub mod index_expression;
pub mod lambda_expression;
pub mod literal;
pub mod literal_expression;
pub mod macro_invocation;
pub mod macro_metavariable;
pub mod match_arm;
pub mod match_expression;
pub mod member_expression;
pub mod path_expression;
pub mod pattern;
pub mod span;
pub mod spawn_expression;
pub mod string_decode;
pub mod struct_literal_expression;
pub mod struct_literal_field;
pub mod try_expression;
pub mod unary_expression;

pub use array_literal_expression::ArrayLiteralExpression;
pub use assign_expression::{AssignExpression, AssignOp};
pub use binary_expression::{BinaryExpression, BinaryOp};
pub use block_expression::BlockExpression;
pub use call_expression::CallExpression;
pub use clif_block::ClifBlockExpression;
pub use code_string::{CodeStringLiteral, CodeStringSegment, materialize_code_segments, parse_plain_code_body};
pub use enum_constructor_expression::EnumConstructorExpression;
pub use expression::Expression;
pub use grouped_expression::GroupedExpression;
pub use index_expression::IndexExpression;
pub use lambda_expression::{LambdaExpression, LambdaParameter};
pub use literal::Literal;
pub use literal::{integer_literal_magnitude, integer_literal_primitive_type};
pub use literal_expression::LiteralExpression;
pub use macro_invocation::MacroInvocation;
pub use macro_metavariable::MacroMetavariable;
pub use match_arm::MatchArm;
pub use match_expression::MatchExpression;
pub use member_expression::MemberExpression;
pub use path_expression::PathExpression;
pub use pattern::{EnumPattern, Pattern};
pub use spawn_expression::SpawnExpression;
pub use string_decode::{
    StringLiteralPart, decode_string_literal_token, split_string_literal_parts, split_string_literal_token,
    try_decode_string_literal, try_decode_string_literal_token,
};
pub use struct_literal_expression::StructLiteralExpression;
pub use struct_literal_field::StructLiteralField;
pub use try_expression::TryExpression;
pub use unary_expression::{UnaryExpression, UnaryOp};
