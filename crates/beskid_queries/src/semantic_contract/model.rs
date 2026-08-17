//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

pub use beskid_abi::runtime_source::CorelibService;
use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::queries::{node_kind, node_span};

/// Source-unit identity, interned by a normalized absolute logical path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug, persist)]
pub struct SourceUnitId {
    #[get(interned_path)]
    #[returns(ref)]
    path: PathBuf,
}

impl SourceUnitId {
    /// Normalize the deepest existing ancestor before interning the remaining logical suffix.
    ///
    /// This makes new LSP files stable when they are first named through a symlink and later
    /// created on disk.
    pub fn new(db: &dyn Db, path: PathBuf) -> Self {
        Self::intern_path(db, normalized_source_path(&path))
    }

    pub fn path(self, db: &dyn Db) -> &PathBuf {
        self.interned_path(db)
    }
}

/// Format a generation-safe syntax key as `path#gN:nN` for traces and diagnostics.
pub fn format_ast_node_key(db: &dyn Db, key: AstNodeKey) -> String {
    key.display_label(key.unit.path(db).display())
}

/// Format a lowering/diagnostic site as `path#gN:nN Construct@line:col-line:col`.
///
/// Falls back to `Unknown` / `?:?-?:?` when kind or span facts are unavailable so the key is
/// still actionable without requiring a second query at the call site.
pub fn format_ast_node_site(db: &dyn Db, key: AstNodeKey) -> String {
    let label = format_ast_node_key(db, key);
    let construct =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "Unknown".to_owned());
    let range = node_span(db, key).ok().flatten().map(format_source_span_range).unwrap_or_else(|| "?:?-?:?".to_owned());
    format!("{label} {construct}@{range}")
}

/// Format a source span as `line:col-line:col` (1-based endpoints).
pub fn format_source_span_range(span: SourceSpan) -> String {
    format!("{}:{}-{}:{}", span.line_col_start.0, span.line_col_start.1, span.line_col_end.0, span.line_col_end.1)
}

/// Format a source-level trace entry as `<source_label>:<line>:<col> (<Construct>)`.
///
/// `source_label` is a short name for the containing source unit (typically the logical path
/// from the assembly). This intentionally omits the generation-safe `#gN:nN` suffix and byte
/// range so that `at`-style traces remain readable under `BESKID_COMPILER_TRACE`.
pub fn format_ast_node_trace(db: &dyn Db, key: AstNodeKey, source_label: &str) -> String {
    let construct =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "Unknown".to_owned());
    let position = node_span(db, key)
        .ok()
        .flatten()
        .map(|span| format!("{}:{}", span.line_col_start.0, span.line_col_start.1))
        .unwrap_or_else(|| "?:?".to_owned());
    format!("{source_label}:{position} ({construct})")
}

#[cfg(test)]
mod ast_node_site_format_tests {
    use super::{SourceSpan, format_source_span_range};

    #[test]
    fn formats_line_column_range() {
        let span = SourceSpan { start: 100, end: 140, line_col_start: (52, 5), line_col_end: (55, 6) };
        assert_eq!(format_source_span_range(span), "52:5-55:6");
    }
}

fn normalized_source_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    };
    let mut ancestor = absolute.clone();
    let mut suffix = Vec::<OsString>::new();

    loop {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(leaf) = ancestor.file_name().map(ToOwned::to_owned) else {
            return absolute;
        };
        suffix.push(leaf);
        if !ancestor.pop() {
            return absolute;
        }
    }
}

/// Generation-safe key for a syntax node in an interned source unit.
pub type AstNodeKey = beskid_analysis::syntax::AstNodeKey<SourceUnitId>;

/// Typed frontend contract passed to later semantic consumers.
#[derive(Clone)]
pub struct TypedProgram {
    pub project: ProjectSession,
    pub entry: SourceUnitId,
    pub generation: SyntaxGenerationId,
    pub assembly: Arc<ProgramAssembly>,
    /// Present only when this program was assembled from the compiler-embedded canonical
    /// runtime corpus. Ordinary user syntax can never manufacture this capability.
    pub runtime_intrinsic_capability: Option<Arc<RuntimeIntrinsicCapability>>,
    /// Present only for the exact compiler-embedded Corelib syscall facade. This is intentionally
    /// separate from canonical runtime intrinsic authority.
    pub corelib_service_capability: Option<Arc<beskid_abi::runtime_source::CorelibServiceCapability>>,
}

/// Authoritative Salsa input for the current syntax generation of one source unit.
#[salsa::input(persist)]
pub struct SyntaxUnitInput {
    pub(crate) project: ProjectSession,
    pub(crate) unit: SourceUnitId,
    #[returns(ref)]
    pub(crate) revision: Arc<SyntaxUnitRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxUnitRevision {
    pub(crate) generation: SyntaxGenerationId,
    pub(crate) expanded_program: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    pub(crate) syntax_index: Arc<beskid_analysis::syntax_query::SyntaxIndex>,
    pub(crate) source_fingerprint: Arc<str>,
    pub(crate) tree_fingerprint: Arc<str>,
    pub(crate) source_fingerprint_history: Arc<[Arc<str>]>,
    pub(crate) tree_fingerprint_history: Arc<[Arc<str>]>,
}

impl SyntaxUnitInput {
    pub(crate) fn generation(self, db: &dyn Db) -> SyntaxGenerationId {
        self.revision(db).generation
    }

    pub(crate) fn expanded_program(
        self,
        db: &dyn Db,
    ) -> &Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>> {
        &self.revision(db).expanded_program
    }

    pub(crate) fn syntax_index(self, db: &dyn Db) -> &Arc<beskid_analysis::syntax_query::SyntaxIndex> {
        &self.revision(db).syntax_index
    }

    pub(crate) fn source_fingerprint(self, db: &dyn Db) -> &Arc<str> {
        &self.revision(db).source_fingerprint
    }

    /// Whether `key` belongs to this authoritative unit revision.
    pub fn accepts_key(self, db: &dyn Db, key: AstNodeKey) -> bool {
        key.is_current(self.unit(db), self.generation(db))
    }
}

/// Resolution fact for an item reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ResolvedItem {
    pub declaration: AstNodeKey,
}

/// Resolution fact for a local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ResolvedLocal {
    pub declaration: AstNodeKey,
}

/// Owner-qualified backend slot for an exact local declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LocalSlot {
    pub owner: AstNodeKey,
    pub index: u32,
}

/// A current-generation mutable local write target proven from syntax alone.
///
/// The fact exists only for a single-segment path that resolves to a mutable `let` binding or
/// mutable function/method parameter. Immutable, non-local, compound, stale, and invalid targets
/// remain unavailable, so ISLE cannot manufacture a write from a bare local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MutableLocalAssignment {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
}

/// One exact outer lexical declaration captured by a lambda or spawned lambda.
///
/// `class` and `span` come from the first captured use site under the lambda in syntax-index
/// order. They preserve capture mode and source identity without reconstructing HIR snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureCapture {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Backend-relevant closure environment facts derived from one lambda expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironment {
    pub parameters: Arc<[AstNodeKey]>,
    pub captures: Arc<[ClosureCapture]>,
}

/// One deterministic capture field in a target-neutral closure environment ABI shape.
///
/// Field order follows the captured declaration's stable owner/node identity and local slot,
/// never hash-map iteration or a later codegen traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironmentField {
    pub capture: ClosureCapture,
    pub abi_type: SemanticTypeId,
}

/// Requirement that a closure environment descriptor carry a runtime pointer map.
///
/// This is intentionally a requirement, not a claim that a descriptor has been emitted. The
/// query layer has no runtime allocation or descriptor-emission authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosurePointerMapRequirement {
    RuntimeDescriptorRequired,
}

/// Deterministic target-neutral ABI shape for a lambda's capture environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironmentAbiShape {
    pub fields: Arc<[ClosureEnvironmentField]>,
    pub pointer_map: ClosurePointerMapRequirement,
}

/// Current implementation status for consuming closure facts in generated lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosureLoweringStatus {
    NotLowered,
}

/// Current implementation status for creating a closure environment at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosureAllocationStatus {
    NotAllocated,
}

/// Generation-bound callable and environment facts for one lambda expression.
///
/// Generic/inferred callable forms remain unavailable. This fact records no generated lowering
/// or runtime allocation; those statuses remain explicit until codegen owns them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureSignature {
    pub lambda: AstNodeKey,
    pub body: AstNodeKey,
    pub callable: ItemSignature,
    pub environment: ClosureEnvironmentAbiShape,
    pub lowering: ClosureLoweringStatus,
    pub allocation: ClosureAllocationStatus,
}

/// Direct lambda call selected by a current call expression.
///
/// Calls through a local closure binding remain unavailable: syntax facts do not infer an
/// allocation, binding flow, or dynamic dispatch target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureCallTarget {
    pub call: AstNodeKey,
    pub lambda: AstNodeKey,
    pub body: AstNodeKey,
    pub callable: ItemSignature,
}

/// Exact callable operand and captures selected by a `spawn` expression.
///
/// Empty-arg `spawn Entry()` sugar stores the entry path (or lambda), not the CallExpression.
/// Non-empty `spawn Entry(args)` keeps the CallExpression so [`spawn_legality`] can reject it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnTarget {
    pub callee: AstNodeKey,
    pub captures: Arc<[ClosureCapture]>,
}

/// Storage provenance derived from the current syntax authority for one captured local use.
///
/// This fact does not establish closure rooting or allocation. It only classifies source values
/// that are safe to transfer by value; native pointers and mutable bindings are conservatively
/// stack references, because moving either across a fiber boundary can expose an invalid or
/// aliased stack location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CaptureStorageClass {
    TransferableValue,
    StackReference,
}

/// Exact declaration, storage provenance, and source use for one captured local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CaptureStorage {
    pub declaration: AstNodeKey,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Deterministic syntax-owned legality failure for one spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SpawnDiagnosticKind {
    TargetNotCallable,
    TargetRequiresArguments,
    /// `spawn Entry(args)` is not a zero-argument fiber entry; only bare callables or
    /// empty-arg `spawn Entry()` sugar (normalized to `Entry`) are legal.
    CalleeArgumentsUnsupported,
    StackReferenceEscapesSpawn,
}

/// One precise diagnostic selected from current syntax facts for a spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnDiagnostic {
    pub kind: SpawnDiagnosticKind,
    pub span: SourceSpan,
    pub capture: Option<CaptureStorage>,
}

/// Authoritative spawn lowering facts and any source-owned legality diagnostics.
///
/// A legal fact contains a zero-argument callable signature result and no diagnostics. Illegal
/// facts retain the target and any proven result so diagnostics and lowering never need legacy
/// HIR snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnLegality {
    pub target: SpawnTarget,
    pub result: Option<SemanticTypeId>,
    pub span: SourceSpan,
    pub diagnostics: Arc<[SpawnDiagnostic]>,
}

/// Source-only validation of whether a spawn target is a legal zero-argument entry.
///
/// This mirrors current legality facts without claiming that a fiber trampoline, closure
/// allocation, or runtime scheduling object has been generated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnEntryValidation {
    pub spawn: AstNodeKey,
    pub target: AstNodeKey,
    pub callable: Option<ItemSignature>,
    pub is_zero_argument_entry: bool,
    pub diagnostics: Arc<[SpawnDiagnostic]>,
}

impl SpawnLegality {
    pub fn is_legal(&self) -> bool {
        self.diagnostics.is_empty() && self.result.is_some()
    }
}

/// Opaque semantic type identity owned by the query layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SemanticTypeId(pub u32);

impl SemanticTypeId {
    pub const UNIT: Self = Self(0);
    pub const BOOL: Self = Self(1);
    pub const I32: Self = Self(2);
    pub const I64: Self = Self(3);
    pub const U8: Self = Self(4);
    pub const F64: Self = Self(5);
    pub const CHAR: Self = Self(6);
    pub const STRING: Self = Self(7);
    /// Pointer-width unsigned integer in Beskid source, represented as ABI `usize`.
    pub const WORD: Self = Self(8);
    /// Opaque native address used only by the canonical runtime source surface.
    pub const POINTER: Self = Self(9);
    /// Bottom type for operations which cannot return normally.
    pub const NEVER: Self = Self(10);

    /// Return this semantic scalar's target-specific ABI size, alignment, and pointer-map class.
    pub fn scalar_abi_layout(self, pointer_width: u8) -> Option<ScalarAbiLayout> {
        let pointer_size = match pointer_width {
            32 => 4,
            64 => 8,
            _ => return None,
        };
        match self {
            Self::BOOL | Self::U8 => Some(ScalarAbiLayout { size: 1, alignment: 1, is_pointer: false }),
            Self::I32 | Self::CHAR => Some(ScalarAbiLayout { size: 4, alignment: 4, is_pointer: false }),
            Self::I64 | Self::F64 => Some(ScalarAbiLayout { size: 8, alignment: 8, is_pointer: false }),
            Self::WORD => Some(ScalarAbiLayout { size: pointer_size, alignment: pointer_size, is_pointer: false }),
            Self::POINTER | Self::STRING => {
                Some(ScalarAbiLayout { size: pointer_size, alignment: pointer_size, is_pointer: true })
            }
            _ => None,
        }
    }

    /// Source-facing type name used in diagnostics and compiler traces.
    ///
    /// Matches the Beskid surface spellings (`string`, `i32`, `unit`, …). Unknown identities
    /// render as `type#N` so traces never invent a fake primitive name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UNIT => "unit",
            Self::BOOL => "bool",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::F64 => "f64",
            Self::CHAR => "char",
            Self::STRING => "string",
            Self::WORD => "word",
            Self::POINTER => "pointer",
            Self::NEVER => "never",
            _ => "type#?",
        }
    }

    /// Format one identity for traces, including a stable `type#N` fallback for unknowns.
    pub fn display_name(self) -> String {
        match self.as_str() {
            "type#?" => format!("type#{}", self.0),
            name => name.to_owned(),
        }
    }
}

impl std::fmt::Display for SemanticTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display_name())
    }
}

/// Mutable storage that owns a replacement produced by canonical array growth.
///
/// Both forms are generation-bound and name storage that codegen can update before releasing the
/// construction root. Unsupported expressions deliberately have no owner fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollectionMutationOwner {
    Local(LocalSlot),
    AggregateField { receiver: LocalSlot, declaration: AstNodeKey, index: u32 },
}

/// Compiler-owned operation selected only from the resolved canonical Core.Collections.Array declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollectionOperation {
    Append { owner: CollectionMutationOwner },
    Capacity,
    Clear,
    RemoveLast,
}

/// Backend-relevant call classification, detached from legacy HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CallLowering {
    Direct(AstNodeKey),
    Dynamic,
    Runtime(RuntimeIntrinsic),
    CorelibService(CorelibService),
}

/// Exact explicit instantiation of a generic source function.
///
/// The invocation keeps its explicit source type-argument syntax. This fact proves that the
/// current generation resolves to one declaration whose generic arity matches those arguments;
/// it never infers a substitution or consults HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallInstantiation {
    pub declaration: AstNodeKey,
    pub argument_count: u8,
    /// Concrete ABI identities supplied by explicit terminal or nominal-receiver type arguments.
    /// This remains source syntax; it never consults HIR-derived substitutions.
    pub arguments: Arc<[SemanticTypeId]>,
}

/// One exact ABI specialization selected by a current generic call expression.
///
/// The declaration remains generation-safe and the ABI signature is derived exclusively from
/// this invocation's syntax arguments.  Consumers use both fields as the item identity, so two
/// distinct instantiations cannot accidentally share one module declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallSpecialization {
    pub declaration: AstNodeKey,
    pub signature: ItemSignature,
    /// Immutable, declaration-ordered bindings used to derive this ABI shape.  Keeping the
    /// environment with the identity is what lets a later body walk substitute `T` in a nested
    /// generic call instead of lowering the declaration once as though `T` were concrete.
    pub substitutions: Arc<[GenericSubstitution]>,
}

/// Exact source-backed application of a generic nominal method receiver.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericNominalMethodReceiver {
    pub method: AstNodeKey,
    pub receiver: AstNodeKey,
    pub owner: AstNodeKey,
    pub substitutions: Arc<[GenericSubstitution]>,
}

/// One concrete binding in a generic specialization environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericSubstitution {
    pub parameter: Arc<str>,
    pub argument: SemanticTypeId,
}

/// A fully materializable generic declaration instance.  This is deliberately detached from a
/// call node: module emission may discover the same instance from several callers but declares
/// exactly one item for its `(declaration, substitutions)` identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericSpecializationInstance {
    pub declaration: AstNodeKey,
    pub signature: ItemSignature,
    pub substitutions: Arc<[GenericSubstitution]>,
}

/// A nested generic call whose source type arguments refer to the enclosing declaration's
/// generic parameters.  `parameter_arguments` are resolved only while walking a concrete
/// [`GenericSpecializationInstance`]; they are never guessed from an uninstantiated body.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallTemplate {
    pub declaration: AstNodeKey,
    /// Declaration-ordered generic parameter names of the nested callee.
    pub parameters: Arc<[Arc<str>]>,
    pub parameter_arguments: Arc<[Arc<str>]>,
}

/// Stable module identity for a materialized generic declaration.
///
/// ABI signatures alone are not sufficient: distinct nominal substitutions may lower to the
/// same pointer ABI. The identity therefore records declaration generation plus every ordered
/// source parameter name and concrete semantic argument.
pub fn generic_specialization_identity(instance: &GenericSpecializationInstance) -> Arc<[u32]> {
    let generation = instance.declaration.generation.0;
    let mut identity = vec![
        (generation >> 32) as u32,
        generation as u32,
        instance.declaration.node.0,
        u32::try_from(instance.substitutions.len()).unwrap_or(u32::MAX),
    ];
    for binding in instance.substitutions.iter() {
        // Encode every UTF-8 byte with a length delimiter. This is deliberately not a hash:
        // distinct source parameter names cannot collide in the module identity.
        identity.push(u32::try_from(binding.parameter.len()).unwrap_or(u32::MAX));
        identity.extend(binding.parameter.bytes().map(u32::from));
        identity.push(binding.argument.0);
    }
    identity.push(u32::MAX);
    identity.extend(instance.signature.parameters.iter().map(|semantic| semantic.0));
    identity.push(instance.signature.result.0);
    identity.into()
}

/// One semantic cast required while lowering an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CastIntent {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PrimitiveNumericConversion {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

/// Control-flow facts established for one AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ControlFlow {
    pub may_fall_through: bool,
}

/// Exact source bounds for the built-in `range(start, end)` form consumed by a `for` loop.
///
/// This is deliberately separate from ordinary call lowering: `range` is syntax sugar for the
/// loop emitter, not a dynamically dispatched function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RangeForFact {
    pub start: AstNodeKey,
    pub end: AstNodeKey,
}

/// Generation-bound iterator declaration and element type for one `ForStatement`.
///
/// The declaration is the loop-variable identifier. Element type is proven only for the
/// syntax-only `range(start, end)` iterable; other iterables remain unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ForIteratorFact {
    pub declaration: AstNodeKey,
    pub element_type: SemanticTypeId,
}

/// Source-proven bulk calling-convention marker for one function or method parameter.
///
/// A `bulk` parameter still lowers as a single array parameter at the callee (the signature
/// shape is unchanged); this fact only marks *which* parameter is bulk and its declared
/// element ABI type, so call-site lowering can pack N scalar arguments into a fresh rooted
/// array before the direct call. The parameter index is the position of the parameter in its
/// enclosing callable's parameter list (declaration order). Stale, unregistered, non-parameter
/// nodes, and parameters without the `bulk` modifier contain no fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BulkParameterFact {
    pub parameter: AstNodeKey,
    pub parameter_index: u32,
    pub element_abi_type: SemanticTypeId,
}

/// Syntax-proven payload/error shapes for one postfix `Result` propagation expression.
///
/// The operand must be a direct, explicitly typed function parameter with the exact
/// `Result<TPayload, TError>` syntax. The enclosing function must return `Result<_, TError>`
/// using the same error syntax. Other propagation forms remain unavailable until they have
/// their own syntax facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TryExpressionFact {
    pub expression: AstNodeKey,
    pub operand: AstNodeKey,
    pub payload_type: SemanticTypeId,
    pub error_type: SemanticTypeId,
    pub enclosing_return: SemanticTypeId,
}

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}

/// Target-neutral storage shape for one source aggregate field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AggregateFieldShape {
    Scalar(SemanticTypeId),
    Nominal(AstNodeKey),
}

/// Source-ordered, named fields of one nominal `type` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AggregateLayoutFact {
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Exact nominal field selected by a direct local receiver field path.
///
/// The receiver must resolve through the current syntax generation to a parameter or explicitly
/// typed local whose nominal declaration has one matching field. More dynamic member shapes
/// intentionally remain unavailable until they have their own syntax authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AggregateFieldAccess {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
    pub index: u32,
}

/// Target-specific ABI layout of one semantic scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ScalarAbiLayout {
    pub size: u64,
    pub alignment: u64,
    pub is_pointer: bool,
}

/// Exact ABI-v5 storage selected by one source enum variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumScalarPayloadVariantLayout {
    pub payload_type: Option<SemanticTypeId>,
    pub payload_offset: Option<u64>,
}

/// Target-specific managed-object layout for an enum whose variants carry at most one scalar value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumScalarPayloadObjectLayout {
    pub object_size: u64,
    pub object_alignment: u64,
    pub tag_offset: u64,
    pub storage_fields: Arc<[(SemanticTypeId, u64)]>,
    pub pointer_map_offsets: Arc<[u64]>,
    pub variants: Arc<[EnumScalarPayloadVariantLayout]>,
}

/// Source-ordered variants and fields of one nominal `enum` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumLayoutFact {
    pub variants: Arc<[EnumVariantLayoutFact]>,
}

/// One source enum variant with its source-ordered named fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumVariantLayoutFact {
    pub name: Arc<str>,
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Exact enum declaration, source-order variant, and payload selected by a constructor.
///
/// The current generated ISLE enum emitter represents at most one payload value per variant.
/// Constructors with more than one source field deliberately remain unavailable instead of
/// silently dropping data while that emitter is extended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumConstructorFact {
    pub declaration: AstNodeKey,
    pub variant_index: u32,
    pub payload: Option<AstNodeKey>,
}

/// One direct identifier payload binding consumed by the generated enum-match emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchBindingFact {
    /// Exact identifier declaration introduced by the match pattern.
    pub declaration: AstNodeKey,
    /// Source-proven ABI shape of the single matched variant payload.
    pub payload: AggregateFieldShape,
}

/// One source arm consumed by the generated enum-match emitter.
///
/// Guards and nested, literal, or multi-payload destructuring remain unavailable until the
/// generated ISLE emitter has explicit representations for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchArmFact {
    pub variant_index: Option<u32>,
    pub body: AstNodeKey,
    pub binding: Option<EnumMatchBindingFact>,
}

/// Exact enum declaration and source-ordered arms selected by a `match` expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchFact {
    pub declaration: AstNodeKey,
    /// Concrete layout selected by an explicitly typed local or parameter scrutinee.
    ///
    /// Keeping this applied source layout on the match fact lets codegen lower generic enums
    /// without reconstructing type arguments from retired HIR artifacts.
    pub layout: EnumLayoutFact,
    pub arms: Arc<[EnumMatchArmFact]>,
}

/// Exact linker symbol declared by a syntax `[Export(Symbol:"...")]` attribute.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExportSymbol(pub Arc<str>);

/// Generation-safe metadata attached to one syntax `test` item.
///
/// The CLI uses this instead of inspecting the legacy assembled program, so discovery and
/// filtering remain tied to the same expanded syntax revision that codegen executes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TestItem {
    pub name: Arc<str>,
    pub qualified_name: Arc<str>,
    pub tags: Arc<[Arc<str>]>,
    pub group: Option<Arc<str>>,
    pub skip_condition: Option<bool>,
    pub skip_reason: Option<Arc<str>>,
    pub selection_span: SourceSpan,
}

/// Trusted runtime operation selected by semantic analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RuntimeIntrinsic(pub u32);

/// Syntactic name of a potential ABI-v5 runtime intrinsic call.
///
/// This is intentionally only a syntax fact. Codegen must pair it with the opaque canonical
/// runtime capability before it may become an import.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RuntimeIntrinsicName(pub Arc<str>);

/// A deterministic completion replacement range in the current source unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CompletionContext {
    pub cursor: usize,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CompletionKind {
    Function,
    Module,
    Variable,
    Type,
    Method,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CompletionCandidate {
    pub label: Arc<str>,
    pub kind: CompletionKind,
    pub detail: Option<Arc<str>>,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

pub type IndexedNodeKind = beskid_analysis::syntax_query::NodeKind;
pub type SourceSpan = beskid_analysis::syntax::SpanInfo;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LiteralFact {
    Integer(Arc<str>),
    Float(Arc<str>),
    String(Arc<str>),
    Char(Arc<str>),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("{message}")]
pub struct SemanticError {
    message: Arc<str>,
    diagnostics: Arc<[Arc<str>]>,
    unavailable: bool,
}

impl SemanticError {
    pub(crate) fn new(message: impl Into<Arc<str>>) -> Self {
        let message = message.into();
        Self { diagnostics: Arc::from([Arc::clone(&message)]), message, unavailable: false }
    }

    pub(crate) fn from_diagnostics(messages: impl IntoIterator<Item = String>) -> Self {
        let diagnostics = messages.into_iter().map(Arc::<str>::from).collect::<Vec<_>>();
        let message = diagnostics.iter().map(AsRef::as_ref).collect::<Vec<_>>().join("\n");
        Self { message: Arc::from(message), diagnostics: diagnostics.into(), unavailable: false }
    }

    pub fn unavailable(query: &str) -> Self {
        let message =
            Arc::<str>::from(format!("semantic query `{query}` is unavailable until its AST/Salsa port is complete"));
        Self { diagnostics: Arc::from([Arc::clone(&message)]), message, unavailable: true }
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }

    pub fn diagnostics(&self) -> &[Arc<str>] {
        &self.diagnostics
    }
}

pub type SemanticQueryResult<T> = Result<Option<T>, SemanticError>;
