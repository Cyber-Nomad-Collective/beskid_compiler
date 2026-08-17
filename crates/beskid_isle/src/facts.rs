use crate::layout::{ArrayLayout, EnumLayout, ManagedArrayAllocation, ManagedStructAllocation, StructLayout};
pub use beskid_queries::AstNodeKey;
use cranelift_codegen::ir::{Signature, Type};
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralKind {
    Integer,
    Float,
    String,
    Char,
    Boolean,
}

/// Compiler-owned primitives available only to canonical runtime syntax.
///
/// They are selected from the manifest-backed capability, never from a user-declared extern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeIntrinsicKind {
    MemoryCopy,
    MemorySet,
    NativeWordFromPointer,
    PointerFromNativeWord,
    PointerAdd,
    RawWordLoad,
    RawWordStore,
    RawByteLoad,
    RawByteStore,
    ArchContextSize(u64),
    ArchContextAlignment(u64),
    SchedulerFiberEntryAddress,
    SchedulerReturnTrampolineAddress,
    SchedulerPollEntryInvoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorFact {
    Or,
    And,
    BitOr,
    BitAnd,
    Shl,
    Shr,
    IdentityEq,
    IdentityNotEq,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Not,
    StringAdd,
    StringEq,
    StringNotEq,
    EnumEq,
    EnumNotEq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexTarget {
    String,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForIterableKind {
    Range,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Direct,
    PrimitiveNumericConversion,
    InlineLambda,
    RuntimeIntrinsic,
    CollectionOperation,
    /// A direct call whose callee declares a `bulk` array parameter.
    ///
    /// The call site packs N scalar arguments into a fresh rooted array (reusing the
    /// `emit_array_literal` allocation sequence) and direct-calls the callee with that array as
    /// its sole argument. The callee signature still has one array parameter, so this is a
    /// distinct lowering path from [`CallKind::Direct`], which requires argument/signature arity
    /// to match.
    Bulk,
    Dynamic,
}

/// Exact semantic call target.
///
/// Source items carry their complete generation-safe syntax key.  A node id is only unique
/// within one source unit and revision, so using it as a module-import key can bind a call to an
/// unrelated item when two units happen to assign the same local id. Runtime intrinsics are not
/// source items and retain their canonical ABI-table index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DirectCallee {
    Item(AstNodeKey),
    /// One generic source declaration paired with its exact call-derived ABI identity.
    ///
    /// The vector stores parameter ABI type identities followed by the result identity.  It is
    /// deliberately structural rather than a lossy hash so module imports cannot conflate two
    /// valid generic instantiations.
    SpecializedItem {
        declaration: AstNodeKey,
        abi_identity: std::sync::Arc<[u32]>,
    },
    RuntimeIntrinsic(u32),
    /// One compiler-authorized Corelib syscall ABI service, identified by its manifest symbol.
    ///
    /// This is intentionally distinct from [`Self::RuntimeIntrinsic`]: Corelib source authority
    /// is not canonical-runtime intrinsic authority and cannot reuse its capability token.
    CorelibService(&'static str),
    /// One generated ABI-v5 fiber entry trampoline, keyed by its source `spawn` expression.
    SpawnTrampoline(AstNodeKey),
    /// One generated ABI-v5 closure entry trampoline, keyed by its source `LambdaExpression`.
    LambdaTrampoline(AstNodeKey),
}

impl DirectCallee {
    pub const fn item(key: AstNodeKey) -> Self {
        Self::Item(key)
    }

    pub fn specialized_item(declaration: AstNodeKey, abi_identity: impl Into<std::sync::Arc<[u32]>>) -> Self {
        Self::SpecializedItem { declaration, abi_identity: abi_identity.into() }
    }

    pub const fn runtime_intrinsic(index: u32) -> Self {
        Self::RuntimeIntrinsic(index)
    }

    pub const fn corelib_service(symbol: &'static str) -> Self {
        Self::CorelibService(symbol)
    }

    pub const fn spawn_trampoline(spawn: AstNodeKey) -> Self {
        Self::SpawnTrampoline(spawn)
    }

    pub const fn lambda_trampoline(lambda: AstNodeKey) -> Self {
        Self::LambdaTrampoline(lambda)
    }
}

/// Exact source entry selected for the first executable spawn lowering leaf.
///
/// Capture-free entries keep a null environment. Capture-proven entries carry artifact-owned
/// allocate/store/root authority; unsupported capture shapes remain unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnEntry {
    pub trampoline: DirectCallee,
    pub closure_environment: Option<InlineClosureEnvironment>,
}

/// One transferable capture field stored into an ABI-v5 closure environment before a call/spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineCaptureField {
    pub local_slot: LocalSlotId,
    pub field_offset: u32,
    pub pointer_map_index: Option<u64>,
    pub value_type: Type,
}

/// Artifact-owned allocate/store/root facts for a capturing immediate call or spawn.
///
/// The symbols name module-local static data. Rooting always uses the current-thread helper; no
/// TLS pointer is ever supplied through this fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineClosureEnvironment {
    pub allocation_request_symbol: std::sync::Arc<str>,
    pub descriptor_symbol: std::sync::Arc<str>,
    pub root_slot_index: u64,
    pub captures: Vec<InlineCaptureField>,
}

/// An immediate lambda call selected from current syntax facts.
///
/// Capture-free calls remain allocation-free. Capturing calls carry ABI-v5 environment authority
/// and otherwise remain unavailable; there is no dynamic closure fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineLambdaCall {
    pub body: AstNodeKey,
    pub parameters: Vec<ParameterSlot>,
    pub result_type: Type,
    pub closure_environment: Option<InlineClosureEnvironment>,
}

/// Exact source entry selected for one freestanding lambda expression lowering leaf.
///
/// Capture-free entries keep a null environment. Capture-proven entries carry artifact-owned
/// allocate/store/root authority; unsupported capture shapes remain unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaEntry {
    pub trampoline: DirectCallee,
    pub closure_environment: Option<InlineClosureEnvironment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallImportError {
    UnknownCallee,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchArmBindingFact {
    pub slot: LocalSlotId,
    pub value_type: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchArmFact {
    pub(crate) discriminant: Option<u64>,
    pub(crate) body: AstNodeKey,
    pub(crate) binding: Option<MatchArmBindingFact>,
}

impl MatchArmFact {
    pub const fn variant(discriminant: u64, body: AstNodeKey) -> Self {
        Self { discriminant: Some(discriminant), body, binding: None }
    }

    pub const fn variant_with_binding(
        discriminant: u64,
        body: AstNodeKey,
        binding: Option<MatchArmBindingFact>,
    ) -> Self {
        Self { discriminant: Some(discriminant), body, binding }
    }

    pub const fn wildcard(body: AstNodeKey) -> Self {
        Self { discriminant: None, body, binding: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeFact {
    pub(crate) start: AstNodeKey,
    pub(crate) end: AstNodeKey,
    pub(crate) step: i64,
    pub(crate) inclusive: bool,
}

impl RangeFact {
    pub const fn new(start: AstNodeKey, end: AstNodeKey, step: i64, inclusive: bool) -> Self {
        Self { start, end, step, inclusive }
    }
}

pub type Unit = ();

/// Scalar facts consumed by leaf ISLE rules.
///
/// The frontend adapter implements this trait with generation-checked Salsa queries. It is a
/// compile-time boundary while those queries are integrated, not a second semantic model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionMutationOwner {
    Local(LocalSlotId),
    AggregateField { receiver: LocalSlotId, field_index: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionOperation {
    Append { owner: CollectionMutationOwner },
    UnprovenMutationOwner,
    Capacity,
    Clear,
    RemoveLast,
}

pub trait NodeFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind>;
    fn literal_kind(&self, _key: AstNodeKey) -> Option<LiteralKind> {
        None
    }
    fn operator_fact(&self, _key: AstNodeKey) -> Option<OperatorFact> {
        None
    }
    fn call_kind(&self, _key: AstNodeKey) -> Option<CallKind> {
        None
    }
    fn primitive_numeric_conversion(
        &self,
        _key: AstNodeKey,
    ) -> Option<(beskid_queries::SemanticTypeId, beskid_queries::SemanticTypeId)> {
        None
    }
    /// Exact semantic type used to validate a primitive conversion fact before it reaches CLIF.
    fn semantic_type(&self, _key: AstNodeKey) -> Option<beskid_queries::SemanticTypeId> {
        None
    }
    /// Syntax/Salsa-proven Result propagation facts for postfix `value?`.
    ///
    /// Implementations must return `None` for stale, foreign, unsupported, or otherwise
    /// unproven nodes so generated ISLE fails closed before CLIF.
    fn try_expression_fact(&self, _key: AstNodeKey) -> Option<beskid_queries::TryExpressionFact> {
        None
    }
    fn runtime_intrinsic_kind(&self, _key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        None
    }
    fn collection_operation(&self, _key: AstNodeKey) -> Option<CollectionOperation> {
        None
    }
    fn collection_element_type(&self, _key: AstNodeKey) -> Option<Type> {
        None
    }
    fn child(&self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
        None
    }
    fn statement_count(&self, _key: AstNodeKey) -> Option<u8> {
        None
    }
    fn block_result(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn let_initializer(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn integer_literal(&self, key: AstNodeKey) -> Option<i64>;
    /// Constant values are immediate and therefore have no local storage slot.
    fn constant_integer(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }
    /// Compiler-minted canonical-runtime constants may be materialized at an
    /// otherwise exact direct-call ABI argument type. Ordinary source never
    /// receives this authority.
    fn canonical_runtime_constant_integer(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }
    fn boolean_literal(&self, _key: AstNodeKey) -> Option<bool> {
        None
    }
    fn float_literal(&self, _key: AstNodeKey) -> Option<f64> {
        None
    }
    fn char_literal(&self, _key: AstNodeKey) -> Option<char> {
        None
    }
    fn string_literal(&self, _key: AstNodeKey) -> Option<Arc<str>> {
        None
    }
    fn scalar_type(&self, key: AstNodeKey) -> Option<Type>;
    fn direct_callee(&self, _key: AstNodeKey) -> Option<DirectCallee> {
        None
    }
    fn call_signature(&self, _key: AstNodeKey) -> Option<Signature> {
        None
    }
    fn call_arguments(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn inline_lambda_call(&self, _key: AstNodeKey) -> Option<InlineLambdaCall> {
        None
    }
    fn array_elements(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn array_layout(&self, _key: AstNodeKey) -> Option<ArrayLayout> {
        None
    }
    fn managed_array_allocation(&self, _key: AstNodeKey) -> Option<ManagedArrayAllocation> {
        None
    }
    fn struct_fields(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn struct_layout(&self, _key: AstNodeKey) -> Option<StructLayout> {
        None
    }
    fn managed_struct_allocation(&self, _key: AstNodeKey) -> Option<ManagedStructAllocation> {
        None
    }
    fn field_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn field_receiver_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    fn enum_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    /// Enum layout suitable for binary comparison: resolves the common enum type of
    /// both operands so discriminant comparison can load the correct tag at the correct
    /// offset. Returns None when either operand is not an enum value.
    fn binary_enum_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    fn enum_variant_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn enum_payload(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn match_arms(&self, _key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        None
    }
    fn range_fact(&self, _key: AstNodeKey) -> Option<RangeFact> {
        None
    }
    fn spawn_entry(&self, _key: AstNodeKey) -> Option<SpawnEntry> {
        None
    }
    fn lambda_entry(&self, _key: AstNodeKey) -> Option<LambdaEntry> {
        None
    }
    fn local_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    /// Proven mutable destination slot for one simple local assignment expression.
    fn mutable_local_assignment_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    fn dispatch_builtin_symbol(&self, _key: AstNodeKey) -> Option<&'static str> {
        None
    }
    fn index_target_is_string(&self, _key: AstNodeKey) -> bool {
        false
    }
    /// Parameter slots in source order for one function item.
    fn function_parameters(&self, _key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        None
    }
    /// Raw body text of a clif block expression.
    fn clif_block_body(&self, _key: AstNodeKey) -> Option<String> {
        None
    }
}

/// Generation-safe local slot and scalar type for one emitted function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalSlotId {
    pub owner_node: u32,
    pub index: u32,
}

/// Generation-safe local slot and scalar type for one emitted function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterSlot {
    pub slot: LocalSlotId,
    pub value_type: Type,
}
