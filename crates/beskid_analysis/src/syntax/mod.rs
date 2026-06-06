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
    ArrayLiteralExpression, AssignExpression, AssignOp, BinaryExpression, BinaryOp,
    BlockExpression, CallExpression, EnumConstructorExpression, EnumPattern, Expression,
    GroupedExpression, IndexExpression, LambdaExpression, LambdaParameter, Literal,
    LiteralExpression, MacroInvocation, MacroMetavariable, MatchArm, MatchExpression,
    MemberExpression, PathExpression, Pattern, SpawnExpression, StructLiteralExpression,
    StructLiteralField, TryExpression, UnaryExpression, UnaryOp,
};
pub use items::{
    Attribute, AttributeArgument, AttributeDeclaration, AttributeParameter, AttributeTarget,
    ContractDefinition, ContractEmbedding, ContractMethodSignature, ContractNode, EnumDefinition,
    EnumVariant, ExtendTypeDefinition, FunctionDefinition, HostBodyItem, HostDefinition,
    InjectQualifier, InlineModule, LaunchStatement, MacroDefinition, MacroFragmentKind,
    MacroParameter, MethodDefinition, ModuleDeclaration, Node, Program, RegistrationLifetime,
    RegistryBlock, RegistryEntry, ScopeDefinition, ScopeHook, ScopeHookKind, TestDefinition,
    TestMetaSection, TestMetadataEntry, TestSkipEntry, TestSkipSection, TypeDefinition,
    UseDeclaration, WithStatement,
};
pub use statements::{
    Block, BreakStatement, ContinueStatement, ElseBranch, ExpressionStatement, ForStatement,
    IfStatement, LetStatement, RangeExpression, ReturnStatement, Statement, WhileStatement,
};
pub use types::{EnumPath, Field, FieldKind, Parameter, Path, PathSegment, PrimitiveType, Type};
