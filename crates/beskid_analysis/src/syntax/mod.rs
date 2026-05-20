//! Parsed concrete syntax for Beskid sources (items, types, statements, expressions).
//!
//! Types here are produced by the pest-based parser and wrapped in [`Spanned`](crate::syntax::Spanned)
//! for source locations. See [`Parsable`](crate::parsing::parsable::Parsable) for parsing entry points.

pub mod common;
pub mod expressions;
pub mod items;
pub mod statements;
pub mod types;

pub use common::{HasSpan, Identifier, SpanInfo, Spanned, Visibility};
pub use expressions::{
    AssignExpression, AssignOp, BinaryExpression, BinaryOp, BlockExpression, CallExpression,
    EnumConstructorExpression, EnumPattern, Expression, GroupedExpression, LambdaExpression,
    LambdaParameter, Literal, LiteralExpression, MatchArm, MatchExpression, MemberExpression,
    PathExpression, Pattern, SpawnExpression, StructLiteralExpression, StructLiteralField,
    TryExpression, UnaryExpression, UnaryOp,
};
pub use items::{
    Attribute, AttributeArgument, AttributeDeclaration, AttributeParameter, AttributeTarget,
    ContractDefinition, ContractEmbedding, ContractMethodSignature, ContractNode, EnumDefinition,
    EnumVariant, ExtendTypeDefinition, FunctionDefinition, InlineModule, MethodDefinition,
    ModuleDeclaration, Node, Program, TestDefinition, TestMetaSection, TestMetadataEntry,
    TestSkipEntry, TestSkipSection, TypeDefinition, UseDeclaration,
};
pub use statements::{
    Block, BreakStatement, ContinueStatement, ExpressionStatement, ForStatement, IfStatement,
    LetStatement, RangeExpression, ReturnStatement, Statement, WhileStatement,
};
pub use types::{
    EnumPath, Field, FieldKind, Parameter, ParameterModifier, Path, PathSegment, PrimitiveType,
    Type,
};
