//! Syntax and HIR tree walks: [`Query`] / [`Walker`](walker::AstWalker), [`Visit`](visit::Visit), and kind enums.
//!
//! The [`node_kinds!`] macro defines [`NodeKind`] and [`HirNodeKind`] for type-filtered traversal.

#[macro_export]
macro_rules! node_kinds {
    ($enum_name:ident; $($name:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $enum_name {
            $($name),+
        }

        impl $enum_name {
            /// Every variant in declaration order.
            ///
            /// This inventory is emitted by the same macro invocation as the enum, so consumers
            /// cannot maintain a drifting handwritten list of syntax kinds.
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
        }
    };
    ($($name:ident),+ $(,)?) => {
        node_kinds!(NodeKind; $($name),+);
    }
}

node_kinds!(
    NodeKind;
    Program,
    Node,
    FunctionDefinition,
    HostDefinition,
    HostBodyItem,
    RegistryBlock,
    RegistryEntry,
    ScopeDefinition,
    ScopeHook,
    WithStatement,
    LaunchStatement,
    MethodDefinition,
    ExtendTypeDefinition,
    TypeDefinition,
    EnumDefinition,
    EnumVariant,
    ContractDefinition,
    TestDefinition,
    TestMetaSection,
    TestMetadataEntry,
    TestSkipSection,
    TestSkipEntry,
    ContractNode,
    ContractMethodSignature,
    ContractEmbedding,
    Attribute,
    AttributeDeclaration,
    AttributeTarget,
    AttributeParameter,
    AttributeArgument,
    ModuleDeclaration,
    InlineModule,
    UseDeclaration,
    Block,
    Statement,
    LetStatement,
    ReturnStatement,
    BreakStatement,
    ContinueStatement,
    WhileStatement,
    ForStatement,
    IfStatement,
    ElseBranch,
    ExpressionStatement,
    RangeExpression,
    Expression,
    AssignExpression,
    BinaryExpression,
    BinaryOp,
    UnaryExpression,
    UnaryOp,
    CallExpression,
    MemberExpression,
    LiteralExpression,
    PathExpression,
    StructLiteralExpression,
    IndexExpression,
    ArrayLiteralExpression,
    CodeStringLiteral,
    CodeStringSegment,
    EnumConstructorExpression,
    BlockExpression,
    GroupedExpression,
    TryExpression,
    SpawnExpression,
    LambdaExpression,
    LambdaParameter,
    MatchExpression,
    MatchArm,
    Pattern,
    EnumPattern,
    Literal,
    Identifier,
    Type,
    Path,
    PathSegment,
    EnumPath,
    Field,
    Parameter,
    PrimitiveType,
    StructLiteralField,
    StringLiteralPart,
    Visibility,
    MacroFragmentKind,
    MacroParameter,
    MacroDefinition,
    MacroInvocation,
    MacroMetavariable,
);

node_kinds!(
    HirNodeKind;
    Program,
    Module,
    Item,
    FunctionDefinition,
    HostDefinition,
    HostBodyItem,
    RegistryBlock,
    RegistryEntry,
    ScopeDefinition,
    ScopeHook,
    MethodDefinition,
    ExtendTypeDefinition,
    TypeDefinition,
    EnumDefinition,
    EnumVariant,
    ContractDefinition,
    TestDefinition,
    TestMetaSection,
    TestMetadataEntry,
    TestSkipSection,
    TestSkipEntry,
    ContractNode,
    ContractMethodSignature,
    ContractEmbedding,
    Attribute,
    AttributeDeclaration,
    AttributeTarget,
    AttributeParameter,
    AttributeArgument,
    ModuleDeclaration,
    InlineModule,
    UseDeclaration,
    Block,
    Statement,
    LetStatement,
    ReturnStatement,
    BreakStatement,
    ContinueStatement,
    WhileStatement,
    ForStatement,
    IfStatement,
    ElseBranch,
    ExpressionStatement,
    RangeExpression,
    Expression,
    AssignExpression,
    BinaryExpression,
    BinaryOp,
    UnaryExpression,
    UnaryOp,
    CallExpression,
    MemberExpression,
    LiteralExpression,
    PathExpression,
    StructLiteralExpression,
    IndexExpression,
    ArrayLiteralExpression,
    CodeStringLiteral,
    EnumConstructorExpression,
    BlockExpression,
    GroupedExpression,
    TryExpression,
    SpawnExpression,
    LambdaExpression,
    LambdaParameter,
    MatchExpression,
    MatchArm,
    Pattern,
    EnumPattern,
    Literal,
    Identifier,
    Type,
    Path,
    PathSegment,
    EnumPath,
    Field,
    Parameter,
    PrimitiveType,
    StructLiteralField,
    Visibility,
);

mod ancestors;
mod ast_node;
mod descendants;
mod dyn_node_ref;
mod hir_descendants;
mod hir_node;
mod hir_node_ref;
mod hir_query;
mod hir_visit;
mod hir_walker;
mod query;
mod syntax_index;
mod syntax_snapshot;
mod traversal_core;
mod visit;
mod walker;

pub use ancestors::Ancestors;
pub use ast_node::{AstNode, NodeRef};
pub use descendants::Descendants;
pub use dyn_node_ref::DynNodeRef;
pub use hir_descendants::HirDescendants;
pub use hir_node::HirNode;
pub use hir_node_ref::HirNodeRef;
pub use hir_query::HirQuery;
pub use hir_visit::HirVisit;
pub use hir_walker::HirWalker;
pub use query::Query;
pub use syntax_index::{SyntaxIndex, SyntaxNodeMetadata};
pub use syntax_snapshot::{SyntaxNodeId, SyntaxSnapshot};
pub use visit::Visit;
pub use walker::AstWalker;
