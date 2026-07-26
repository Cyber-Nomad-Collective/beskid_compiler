//! Parsed concrete syntax for Beskid sources (items, types, statements, expressions).
//!
//! Types here are produced by the pest-based parser and wrapped in [`Spanned`](crate::syntax::Spanned)
//! for source locations. See [`Parsable`](crate::parsing::parsable::Parsable) for parsing entry points.

pub mod common;
pub mod expressions;
mod identity;
pub mod items;
pub mod statements;
pub mod types;

pub use common::{HasSpan, Identifier, SpanInfo, Spanned, Visibility};
pub use expressions::{
    ArrayLiteralExpression, AssignExpression, AssignOp, BinaryExpression, BinaryOp, BlockExpression, CallExpression,
    CodeStringLiteral, CodeStringSegment, EnumConstructorExpression, EnumPattern, Expression, GroupedExpression,
    IndexExpression, LambdaExpression, LambdaParameter, Literal, LiteralExpression, MacroInvocation, MacroMetavariable,
    MatchArm, MatchExpression, MemberExpression, PathExpression, Pattern, SpawnExpression, StringLiteralPart,
    StructLiteralExpression, StructLiteralField, TryExpression, UnaryExpression, UnaryOp, decode_string_literal_token,
    materialize_code_segments, parse_plain_code_body, split_string_literal_parts, split_string_literal_token,
    try_decode_string_literal, try_decode_string_literal_token,
};
pub use identity::{AstNodeId, AstNodeKey, SyntaxGenerationId};
pub use items::{
    Attribute, AttributeArgument, AttributeDeclaration, AttributeParameter, AttributeTarget, ConstantDefinition, ContractDefinition,
    ContractEmbedding, ContractMethodSignature, ContractNode, EnumDefinition, EnumVariant, ExtendTypeDefinition,
    FunctionDefinition, HostBodyItem, HostDefinition, InjectQualifier, InlineModule, LaunchStatement, MacroDefinition,
    MacroFragmentKind, MacroParameter, MethodDefinition, ModuleDeclaration, Node, Program, RegistrationLifetime,
    RegistryBlock, RegistryEntry, ScopeDefinition, ScopeHook, ScopeHookKind, TestDefinition, TestMetaSection,
    TestMetadataEntry, TestSkipEntry, TestSkipSection, TypeDefinition, UseDeclaration, WithStatement,
};
pub use statements::{
    Block, BreakStatement, ContinueStatement, ElseBranch, ExpressionStatement, ForStatement, IfStatement, LetStatement,
    RangeExpression, ReturnStatement, Statement, WhileStatement,
};
pub use types::{EnumPath, Field, FieldKind, Parameter, Path, PathSegment, PrimitiveType, Type};
