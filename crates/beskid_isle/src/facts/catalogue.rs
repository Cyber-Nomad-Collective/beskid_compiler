//! Syntax node kind catalogue and typed-operation support classification.

macro_rules! node_kinds {
    ($($name:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum NodeKind {
            $($name),+
        }

        impl NodeKind {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
        }
    };
}

node_kinds!(
    Program,
    FunctionDefinition,
    TestDefinition,
    MethodDefinition,
    ExpressionStatement,
    ReturnStatement,
    LetStatement,
    ScopedUseStatement,
    IfStatement,
    WhileStatement,
    BreakStatement,
    ContinueStatement,
    LiteralExpression,
    GroupedExpression,
    UnaryExpression,
    BinaryExpression,
    AssignExpression,
    CallExpression,
    PathExpression,
    IndexExpression,
    ArrayLiteralExpression,
    FieldExpression,
    StructLiteralExpression,
    EnumLiteralExpression,
    MatchExpression,
    RangeExpression,
    BlockExpression,
    ForStatement,
    SpawnExpression,
    LambdaExpression,
    TryExpression,
    ClifBlock,
);

/// Exhaustive disposition of an expanded-syntax kind at the generated ISLE boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxNodeClassification {
    IsleLowered(NodeKind),
    Structural,
    UnsupportedTypedOperation,
}

/// Classify every authoritative expanded-syntax kind without a fallback arm.
pub const fn classify_syntax_node_kind(kind: beskid_queries::IndexedNodeKind) -> SyntaxNodeClassification {
    use SyntaxNodeClassification::{IsleLowered, Structural, UnsupportedTypedOperation};
    use beskid_queries::IndexedNodeKind as Syntax;

    match kind {
        Syntax::Program => IsleLowered(NodeKind::Program),
        Syntax::FunctionDefinition => IsleLowered(NodeKind::FunctionDefinition),
        Syntax::TestDefinition => IsleLowered(NodeKind::TestDefinition),
        // Methods lower as executable items through the same FunctionEmitter path as functions;
        // they are not FunctionDefinition aliases because the body is not child index 0.
        Syntax::MethodDefinition => IsleLowered(NodeKind::MethodDefinition),
        Syntax::ExpressionStatement => IsleLowered(NodeKind::ExpressionStatement),
        Syntax::ReturnStatement => IsleLowered(NodeKind::ReturnStatement),
        Syntax::LetStatement => IsleLowered(NodeKind::LetStatement),
        Syntax::ScopedUseStatement => IsleLowered(NodeKind::ScopedUseStatement),
        Syntax::IfStatement => IsleLowered(NodeKind::IfStatement),
        Syntax::WhileStatement => IsleLowered(NodeKind::WhileStatement),
        Syntax::BreakStatement => IsleLowered(NodeKind::BreakStatement),
        Syntax::ContinueStatement => IsleLowered(NodeKind::ContinueStatement),
        Syntax::LiteralExpression | Syntax::Literal => IsleLowered(NodeKind::LiteralExpression),
        Syntax::GroupedExpression => IsleLowered(NodeKind::GroupedExpression),
        Syntax::UnaryExpression => IsleLowered(NodeKind::UnaryExpression),
        Syntax::BinaryExpression => IsleLowered(NodeKind::BinaryExpression),
        Syntax::AssignExpression => IsleLowered(NodeKind::AssignExpression),
        Syntax::CallExpression => IsleLowered(NodeKind::CallExpression),
        Syntax::PathExpression => IsleLowered(NodeKind::PathExpression),
        Syntax::IndexExpression => IsleLowered(NodeKind::IndexExpression),
        Syntax::ArrayLiteralExpression => IsleLowered(NodeKind::ArrayLiteralExpression),
        Syntax::MemberExpression => IsleLowered(NodeKind::FieldExpression),
        Syntax::StructLiteralExpression => IsleLowered(NodeKind::StructLiteralExpression),
        Syntax::EnumConstructorExpression => IsleLowered(NodeKind::EnumLiteralExpression),
        Syntax::MatchExpression => IsleLowered(NodeKind::MatchExpression),
        Syntax::RangeExpression => IsleLowered(NodeKind::RangeExpression),
        Syntax::Block | Syntax::BlockExpression => IsleLowered(NodeKind::BlockExpression),
        Syntax::ForStatement => IsleLowered(NodeKind::ForStatement),
        Syntax::SpawnExpression => IsleLowered(NodeKind::SpawnExpression),
        Syntax::LambdaExpression => IsleLowered(NodeKind::LambdaExpression),
        Syntax::TryExpression => IsleLowered(NodeKind::TryExpression),
        Syntax::ClifBlockExpression => IsleLowered(NodeKind::ClifBlock),

        Syntax::HostDefinition
        | Syntax::RegistryBlock
        | Syntax::RegistryEntry
        | Syntax::ScopeDefinition
        | Syntax::ScopeHook
        | Syntax::WithStatement
        | Syntax::LaunchStatement
        | Syntax::CodeStringLiteral => UnsupportedTypedOperation,

        Syntax::Node
        | Syntax::ConstantDefinition
        | Syntax::HostBodyItem
        | Syntax::ExtendTypeDefinition
        | Syntax::ImplBlock
        | Syntax::TypeDefinition
        | Syntax::EnumDefinition
        | Syntax::EnumVariant
        | Syntax::ContractDefinition
        | Syntax::TestMetaSection
        | Syntax::TestMetadataEntry
        | Syntax::TestSkipSection
        | Syntax::TestSkipEntry
        | Syntax::ContractNode
        | Syntax::ContractMethodSignature
        | Syntax::ContractEmbedding
        | Syntax::ContractAssociatedType
        | Syntax::AssociatedTypeBinding
        | Syntax::Attribute
        | Syntax::AttributeDeclaration
        | Syntax::AttributeTarget
        | Syntax::AttributeParameter
        | Syntax::AttributeArgument
        | Syntax::ModuleDeclaration
        | Syntax::InlineModule
        | Syntax::UseDeclaration
        | Syntax::Statement
        | Syntax::ElseBranch
        | Syntax::Expression
        | Syntax::BinaryOp
        | Syntax::UnaryOp
        | Syntax::CodeStringSegment
        | Syntax::LambdaParameter
        | Syntax::MatchArm
        | Syntax::Pattern
        | Syntax::EnumPattern
        | Syntax::Identifier
        | Syntax::Type
        | Syntax::Path
        | Syntax::PathSegment
        | Syntax::EnumPath
        | Syntax::Field
        | Syntax::Parameter
        | Syntax::PrimitiveType
        | Syntax::StructLiteralField
        | Syntax::StringLiteralPart
        | Syntax::Visibility
        | Syntax::MacroFragmentKind
        | Syntax::MacroParameter
        | Syntax::MacroDefinition
        | Syntax::MacroInvocation
        | Syntax::MacroMetavariable => Structural,
    }
}

/// Deterministic catalogue in the authoritative syntax declaration order.
pub fn syntax_node_kind_catalogue()
-> impl ExactSizeIterator<Item = (beskid_queries::IndexedNodeKind, SyntaxNodeClassification)> {
    beskid_queries::IndexedNodeKind::ALL.iter().copied().map(|kind| (kind, classify_syntax_node_kind(kind)))
}

/// Authoritative roster of executable syntax forms rejected at the generated ISLE boundary.
///
/// This list must stay equal to every [`SyntaxNodeClassification::UnsupportedTypedOperation`]
/// entry in [`syntax_node_kind_catalogue`] — no silent catch-all arm may hide new kinds.
///
/// For Beskid 0.4 these kinds are intentionally release-rejected (not pending ports):
/// host composition declarations and `with`/`launch` wait on composition-container facts;
/// fenced `code` strings stay unsupported in both paths. `MethodDefinition`, `SpawnExpression`,
/// `LambdaExpression`, and syntax-proven `TryExpression` are production-supported
/// [`IsleLowered`][SyntaxNodeClassification::IsleLowered] forms outside this roster.
pub const UNSUPPORTED_TYPED_OPERATION_KINDS: &[beskid_queries::IndexedNodeKind] = &[
    beskid_queries::IndexedNodeKind::HostDefinition,
    beskid_queries::IndexedNodeKind::RegistryBlock,
    beskid_queries::IndexedNodeKind::RegistryEntry,
    beskid_queries::IndexedNodeKind::ScopeDefinition,
    beskid_queries::IndexedNodeKind::ScopeHook,
    beskid_queries::IndexedNodeKind::WithStatement,
    beskid_queries::IndexedNodeKind::LaunchStatement,
    beskid_queries::IndexedNodeKind::CodeStringLiteral,
];

/// Syntax kinds currently classified as unsupported typed operations, in catalogue order.
pub fn unsupported_typed_operation_kinds() -> impl Iterator<Item = beskid_queries::IndexedNodeKind> {
    syntax_node_kind_catalogue().filter_map(|(kind, classification)| {
        matches!(classification, SyntaxNodeClassification::UnsupportedTypedOperation).then_some(kind)
    })
}
