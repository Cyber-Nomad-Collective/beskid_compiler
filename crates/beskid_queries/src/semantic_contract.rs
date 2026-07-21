//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use beskid_abi::runtime_source::CorelibService;
use beskid_abi::{
    abi_v5::{AbiManifestV5, AbiType, TargetMetadata},
    dispatch_route_for_symbol,
    runtime_source::RuntimeIntrinsicCapability,
};
use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::db::Db;
use crate::inputs::ProjectSession;

/// Source-unit identity, interned by a normalized absolute logical path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug)]
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
    let construct = node_kind(db, key)
        .ok()
        .flatten()
        .map(|kind| format!("{kind:?}"))
        .unwrap_or_else(|| "Unknown".to_owned());
    let range = node_span(db, key)
        .ok()
        .flatten()
        .map(format_source_span_range)
        .unwrap_or_else(|| "?:?-?:?".to_owned());
    format!("{label} {construct}@{range}")
}

/// Format a source span as `line:col-line:col` (1-based endpoints).
pub fn format_source_span_range(span: SourceSpan) -> String {
    format!(
        "{}:{}-{}:{}",
        span.line_col_start.0, span.line_col_start.1, span.line_col_end.0, span.line_col_end.1
    )
}

#[cfg(test)]
mod ast_node_site_format_tests {
    use super::{SourceSpan, format_source_span_range};

    #[test]
    fn formats_line_column_range() {
        let span = SourceSpan {
            start: 100,
            end: 140,
            line_col_start: (52, 5),
            line_col_end: (55, 6),
        };
        assert_eq!(format_source_span_range(span), "52:5-55:6");
    }
}

fn normalized_source_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
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
    pub assembly: Arc<SyntaxProgramAssembly>,
    /// Present only when this program was assembled from the compiler-embedded canonical
    /// runtime corpus. Ordinary user syntax can never manufacture this capability.
    pub runtime_intrinsic_capability: Option<Arc<RuntimeIntrinsicCapability>>,
    /// Present only for the exact compiler-embedded Corelib syscall facade. This is intentionally
    /// separate from canonical runtime intrinsic authority.
    pub corelib_service_capability:
        Option<Arc<beskid_abi::runtime_source::CorelibServiceCapability>>,
}

/// Authoritative Salsa input for the current syntax generation of one source unit.
#[salsa::input]
pub struct SyntaxUnitInput {
    pub(crate) project: ProjectSession,
    pub(crate) unit: SourceUnitId,
    #[returns(ref)]
    pub(crate) revision: Arc<SyntaxUnitRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxUnitRevision {
    pub(crate) generation: SyntaxGenerationId,
    pub(crate) expanded_program:
        Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
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

    pub(crate) fn syntax_index(
        self,
        db: &dyn Db,
    ) -> &Arc<beskid_analysis::syntax_query::SyntaxIndex> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedItem {
    pub declaration: AstNodeKey,
}

/// Resolution fact for a local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedLocal {
    pub declaration: AstNodeKey,
}

/// Owner-qualified backend slot for an exact local declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalSlot {
    pub owner: AstNodeKey,
    pub index: u32,
}

/// A current-generation mutable local write target proven from syntax alone.
///
/// The fact exists only for a single-segment path that resolves to a mutable `let` binding or
/// mutable function/method parameter. Immutable, non-local, compound, stale, and invalid targets
/// remain unavailable, so ISLE cannot manufacture a write from a bare local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MutableLocalAssignment {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
}

/// One exact outer lexical declaration captured by a lambda or spawned lambda.
///
/// `class` and `span` come from the first captured use site under the lambda in syntax-index
/// order. They preserve capture mode and source identity without reconstructing HIR snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClosureCapture {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Backend-relevant closure environment facts derived from one lambda expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClosureEnvironment {
    pub parameters: Arc<[AstNodeKey]>,
    pub captures: Arc<[ClosureCapture]>,
}

/// One deterministic capture field in a target-neutral closure environment ABI shape.
///
/// Field order follows the captured declaration's stable owner/node identity and local slot,
/// never hash-map iteration or a later codegen traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClosureEnvironmentField {
    pub capture: ClosureCapture,
    pub abi_type: SemanticTypeId,
}

/// Requirement that a closure environment descriptor carry a runtime pointer map.
///
/// This is intentionally a requirement, not a claim that a descriptor has been emitted. The
/// query layer has no runtime allocation or descriptor-emission authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosurePointerMapRequirement {
    RuntimeDescriptorRequired,
}

/// Deterministic target-neutral ABI shape for a lambda's capture environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClosureEnvironmentAbiShape {
    pub fields: Arc<[ClosureEnvironmentField]>,
    pub pointer_map: ClosurePointerMapRequirement,
}

/// Current implementation status for consuming closure facts in generated lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosureLoweringStatus {
    NotLowered,
}

/// Current implementation status for creating a closure environment at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosureAllocationStatus {
    NotAllocated,
}

/// Generation-bound callable and environment facts for one lambda expression.
///
/// Generic/inferred callable forms remain unavailable. This fact records no generated lowering
/// or runtime allocation; those statuses remain explicit until codegen owns them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureStorageClass {
    TransferableValue,
    StackReference,
}

/// Exact declaration, storage provenance, and source use for one captured local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureStorage {
    pub declaration: AstNodeKey,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Deterministic syntax-owned legality failure for one spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnDiagnosticKind {
    TargetNotCallable,
    TargetRequiresArguments,
    /// `spawn Entry(args)` is not a zero-argument fiber entry; only bare callables or
    /// empty-arg `spawn Entry()` sugar (normalized to `Entry`) are legal.
    CalleeArgumentsUnsupported,
    StackReferenceEscapesSpawn,
}

/// One precise diagnostic selected from current syntax facts for a spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Backend-relevant call classification, detached from legacy HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericCallSpecialization {
    pub declaration: AstNodeKey,
    pub signature: ItemSignature,
}

/// One semantic cast required while lowering an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastIntent {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

/// Control-flow facts established for one AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlFlow {
    pub may_fall_through: bool,
}

/// Exact source bounds for the built-in `range(start, end)` form consumed by a `for` loop.
///
/// This is deliberately separate from ordinary call lowering: `range` is syntax sugar for the
/// loop emitter, not a dynamically dispatched function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RangeForFact {
    pub start: AstNodeKey,
    pub end: AstNodeKey,
}

/// Generation-bound iterator declaration and element type for one `ForStatement`.
///
/// The declaration is the loop-variable identifier. Element type is proven only for the
/// syntax-only `range(start, end)` iterable; other iterables remain unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForIteratorFact {
    pub declaration: AstNodeKey,
    pub element_type: SemanticTypeId,
}

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}

/// Target-neutral storage shape for one source aggregate field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregateFieldShape {
    Scalar(SemanticTypeId),
    Nominal(AstNodeKey),
}

/// Source-ordered, named fields of one nominal `type` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AggregateLayoutFact {
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Exact nominal field selected by a direct local receiver field path.
///
/// The receiver must resolve through the current syntax generation to a parameter or explicitly
/// typed local whose nominal declaration has one matching field. More dynamic member shapes
/// intentionally remain unavailable until they have their own syntax authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AggregateFieldAccess {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
    pub index: u32,
}

/// Source-ordered variants and fields of one nominal `enum` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumLayoutFact {
    pub variants: Arc<[EnumVariantLayoutFact]>,
}

/// One source enum variant with its source-ordered named fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariantLayoutFact {
    pub name: Arc<str>,
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Exact enum declaration, source-order variant, and payload selected by a constructor.
///
/// The current generated ISLE enum emitter represents at most one payload value per variant.
/// Constructors with more than one source field deliberately remain unavailable instead of
/// silently dropping data while that emitter is extended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumConstructorFact {
    pub declaration: AstNodeKey,
    pub variant_index: u32,
    pub payload: Option<AstNodeKey>,
}

/// One source arm consumed by the generated enum-match emitter.
///
/// Payload destructuring and guards intentionally remain unavailable until the generated ISLE
/// emitter has an equally explicit binding and guard representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumMatchArmFact {
    pub variant_index: Option<u32>,
    pub body: AstNodeKey,
}

/// Exact enum declaration and source-ordered arms selected by a `match` expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumMatchFact {
    pub declaration: AstNodeKey,
    pub arms: Arc<[EnumMatchArmFact]>,
}

/// Exact linker symbol declared by a syntax `[Export(Symbol:"...")]` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSymbol(pub Arc<str>);

/// Generation-safe metadata attached to one syntax `test` item.
///
/// The CLI uses this instead of inspecting the legacy assembled program, so discovery and
/// filtering remain tied to the same expanded syntax revision that codegen executes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeIntrinsic(pub u32);

/// Syntactic name of a potential ABI-v5 runtime intrinsic call.
///
/// This is intentionally only a syntax fact. Codegen must pair it with the opaque canonical
/// runtime capability before it may become an import.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeIntrinsicName(pub Arc<str>);

/// A deterministic completion replacement range in the current source unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompletionContext {
    pub cursor: usize,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Function,
    Module,
    Variable,
    Type,
    Method,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionCandidate {
    pub label: Arc<str>,
    pub kind: CompletionKind,
    pub detail: Option<Arc<str>>,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

pub type IndexedNodeKind = beskid_analysis::syntax_query::NodeKind;
pub type SourceSpan = beskid_analysis::syntax::SpanInfo;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralFact {
    Integer(Arc<str>),
    Float(Arc<str>),
    String(Arc<str>),
    Char(Arc<str>),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatorFact {
    Or,
    And,
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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SemanticError {
    message: Arc<str>,
    diagnostics: Arc<[Arc<str>]>,
    unavailable: bool,
}

impl SemanticError {
    pub(crate) fn new(message: impl Into<Arc<str>>) -> Self {
        let message = message.into();
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable: false,
        }
    }

    pub(crate) fn from_diagnostics(messages: impl IntoIterator<Item = String>) -> Self {
        let diagnostics = messages
            .into_iter()
            .map(Arc::<str>::from)
            .collect::<Vec<_>>();
        let message = diagnostics
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            message: Arc::from(message),
            diagnostics: diagnostics.into(),
            unavailable: false,
        }
    }

    pub fn unavailable(query: &str) -> Self {
        let message = Arc::<str>::from(format!(
            "semantic query `{query}` is unavailable until its AST/Salsa port is complete"
        ));
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable: true,
        }
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }

    pub fn diagnostics(&self) -> &[Arc<str>] {
        &self.diagnostics
    }
}

pub type SemanticQueryResult<T> = Result<Option<T>, SemanticError>;

fn with_registered_syntax<T>(
    db: &dyn Db,
    key: AstNodeKey,
    query: impl FnOnce(&dyn Db, SyntaxUnitInput, AstNodeKey) -> SemanticQueryResult<T>,
) -> SemanticQueryResult<T> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    query(db, syntax, key)
}

#[salsa::tracked]
fn resolved_item_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedItem> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        resolve_item_declaration(db, program, index, key, &path.path.node)
            .map(|declaration| ResolvedItem { declaration })
    })
}

fn resolve_item_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    // A generic receiver remains an exact module/type namespace fact: `Channel<i64>.Create`
    // names the `Create` item imported from `Concurrency.Channel`.  Only a generic terminal
    // callee would require unimplemented function monomorphization.
    if path
        .segments
        .last()
        .is_some_and(|segment| !segment.node.type_args.is_empty())
    {
        return None;
    }
    resolve_item_declaration_candidate(db, program, index, key, path)
}

/// Resolve a function declaration without accepting terminal generic syntax as a call fact.
/// Callers must validate an explicit terminal instantiation through
/// [`generic_call_instantiation`] before treating this candidate as callable.
fn resolve_item_declaration_candidate(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, module_path) = path.segments.split_last()?;
    if module_path.is_empty() {
        let name = name.node.name.node.name.as_str();
        if resolve_lexical_declaration(program, index, key.node, name).is_some() {
            return None;
        }
        return resolve_unqualified_item_declaration(program, index, key, name)
            .or_else(|| unique_function_in_unit(db, key.unit, key.generation, name))
            .or_else(|| unique_imported_function(db, key, name));
    }
    let module_path = module_path
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect::<Vec<_>>();
    let Some(target_unit) = resolve_qualified_module_unit(db, key, &module_path) else {
        return resolve_type_qualified_imported_function(db, key, path);
    };
    unique_exported_function_in_unit(db, target_unit, key.generation, &name.node.name.node.name)
}

/// Resolve a qualified module path from an exact current import and, when required, explicit
/// public child-module edges. Private imports remain available only inside their owner.
fn resolve_qualified_module_unit(
    db: &dyn Db,
    key: AstNodeKey,
    module_path: &[String],
) -> Option<SourceUnitId> {
    let initial = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))?
        .iter()
        .filter_map(|import| {
            import_path_prefix_len(import, module_path).map(|consumed| (import.target, consumed))
        })
        .collect::<Vec<_>>();

    let mut resolved = Vec::new();
    for (unit, consumed) in initial {
        let mut pending = vec![(unit, consumed)];
        let mut visited = std::collections::HashSet::new();
        while let Some((current, consumed)) = pending.pop() {
            if !visited.insert((current, consumed)) {
                continue;
            }
            if consumed == module_path.len() {
                if !resolved.contains(&current) {
                    resolved.push(current);
                }
                continue;
            }
            let segment = &module_path[consumed];
            pending.extend(
                public_module_routes(db, current, key.generation)
                    .into_iter()
                    .filter_map(|(binding, child)| {
                        (binding == *segment).then_some((child, consumed + 1))
                    }),
            );
        }
    }
    let [unit] = resolved.as_slice() else {
        return None;
    };
    Some(*unit)
}

fn import_path_prefix_len(
    import: &crate::db::SyntaxImport,
    module_path: &[String],
) -> Option<usize> {
    // A source unit owns only the names bound by its own `use` declarations. Original import
    // paths may be used only for an unaliased import, where the terminal path segment is that
    // binding, and only for the target module itself. Registry suffixes or child routes would
    // bypass aliases and make visibility order-dependent.
    module_path
        .first()
        .filter(|segment| import.binding == **segment)
        .map(|_| 1)
        .or_else(|| {
            (!import.has_explicit_alias
                && import.binding == *import.path.last()?
                && module_path == import.path.as_slice())
            .then_some(import.path.len())
        })
}

/// Return public routes with their bindings: a target may be re-exported under multiple aliases.
fn public_module_routes(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
) -> Vec<(String, SourceUnitId)> {
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(unit, generation))
        .into_iter()
        .flatten()
        .filter(|import| import.public)
        .map(|import| (import.binding.clone(), import.target))
        .collect()
}

/// Resolve `Imported.ModuleType.Function()` only when the import identifies one source unit,
/// the extra qualifier identifies one declared type in that unit, and the terminal function is
/// likewise unique. This preserves a direct call edge for the Corelib convention of spelling
/// static functions through their nominal type without treating arbitrary nested paths as
/// callable.
fn resolve_type_qualified_imported_function(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (function, module_path) = path.segments.split_last()?;
    let (type_segment, import_path) = module_path.split_last()?;
    let import_path = import_path
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect::<Vec<_>>();
    let target_unit = resolve_qualified_module_unit(db, key, &import_path)?;
    unique_exported_type_in_unit(
        db,
        target_unit,
        key.generation,
        &type_segment.node.name.node.name,
        type_segment.node.type_args.len(),
    )?;
    unique_exported_function_in_unit(
        db,
        target_unit,
        key.generation,
        &function.node.name.node.name,
    )
}

/// Resolve a public module member through its defining syntax unit or an explicit public
/// re-export. This is intentionally limited to assembly-registered `pub use` edges, so a
/// private implementation import cannot become visible through its parent module.
fn unique_exported_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(candidate) = unique_public_function_in_unit(db, current, generation, name) {
            candidates.push(candidate);
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    let [candidate] = candidates.as_slice() else {
        return None;
    };
    Some(*candidate)
}

fn public_reexport_units(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
) -> Vec<SourceUnitId> {
    public_module_routes(db, unit, generation)
        .into_iter()
        .map(|(_, target)| target)
        .fold(Vec::new(), |mut targets, target| {
            if !targets.contains(&target) {
                targets.push(target);
            }
            targets
        })
}

fn unique_public_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let candidates = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                .is_some_and(|function| {
                    function.visibility.node == beskid_analysis::syntax::Visibility::Public
                        && function.name.node.name == name
                })
        })
        .collect::<Vec<_>>();
    let [node] = candidates.as_slice() else {
        return None;
    };
    Some(AstNodeKey {
        unit,
        generation,
        node: *node,
    })
}

/// Resolve an exact function name only when the syntax unit has one unambiguous definition.
/// This preserves reachability for macro-expanded items whose synthetic nodes no longer retain
/// their original module ancestry.
fn unique_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let candidates = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                .is_some_and(|function| function.name.node.name == name)
        })
        .collect::<Vec<_>>();
    let [node] = candidates.as_slice() else {
        return None;
    };
    Some(AstNodeKey {
        unit,
        generation,
        node: *node,
    })
}

/// Resolve an unqualified imported function only when its assembled import targets provide one
/// exact declaration. Arbitrary unresolved bare names deliberately remain unavailable.
fn unique_imported_function(db: &dyn Db, key: AstNodeKey, name: &str) -> Option<AstNodeKey> {
    let targets = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))?
        .iter()
        .map(|import| import.target)
        .fold(Vec::new(), |mut targets, target| {
            if !targets.contains(&target) {
                targets.push(target);
            }
            targets
        });
    let candidates = targets
        .into_iter()
        .filter_map(|target| unique_exported_function_in_unit(db, target, key.generation, name))
        .collect::<Vec<_>>();
    let [declaration] = candidates.as_slice() else {
        return None;
    };
    Some(*declaration)
}

fn resolve_unqualified_item_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    name: &str,
) -> Option<AstNodeKey> {
    if resolve_lexical_declaration(program, index, key.node, name).is_some() {
        return None;
    }

    let mut scope = module_scope(index, key.node)?;
    loop {
        let candidates = index
            .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
            .filter(|candidate| {
                module_scope(index, *candidate) == Some(scope)
                    && index
                        .node_at(program, *candidate)
                        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                        .is_some_and(|function| function.name.node.name == name)
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [declaration] => {
                return Some(AstNodeKey {
                    node: *declaration,
                    ..key
                });
            }
            [] => {}
            _ => return None,
        }
        scope = outer_module_scope(index, scope)?;
    }
}

fn module_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    nearest_ancestor(index, node, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::InlineModule
                | beskid_analysis::syntax_query::NodeKind::Program
        )
    })
}

fn outer_module_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    scope: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    module_scope(index, parent_node(index, scope)?)
}

#[salsa::tracked]
fn resolved_local_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedLocal> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(
            program,
            index,
            key.node,
            segment.node.name.node.name.as_str(),
        )?;
        Some(ResolvedLocal {
            declaration: AstNodeKey {
                node: declaration,
                ..key
            },
        })
    })
}

#[salsa::tracked]
fn local_slot_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<LocalSlot> {
    with_node(db, syntax, key, |_program, index, node| {
        node.of::<beskid_analysis::syntax::Identifier>()?;
        local_slot_for_declaration(index, key)
    })?
    .transpose()
}

#[salsa::tracked]
fn mutable_local_assignment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<MutableLocalAssignment> {
    with_node(db, syntax, key, |program, index, node| {
        let assignment = node.of::<beskid_analysis::syntax::AssignExpression>()?;
        if !matches!(
            assignment.op.node,
            beskid_analysis::syntax::AssignOp::Assign
        ) {
            return None;
        }
        let beskid_analysis::syntax::Expression::Path(path) = &assignment.target.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(
            program,
            index,
            key.node,
            segment.node.name.node.name.as_str(),
        )?;
        if !local_declaration_is_mutable(program, index, declaration) {
            return Some(Err(SemanticError::unavailable("mutable_local_assignment")));
        }
        let declaration = AstNodeKey {
            node: declaration,
            ..key
        };
        let slot = local_slot_for_declaration(index, declaration)?;
        Some(slot.map(|slot| MutableLocalAssignment { declaration, slot }))
    })?
    .transpose()
}

fn local_slot_for_declaration(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<LocalSlot, SemanticError>> {
    let owner = local_declaration_owner(index, key.node)?;
    let slot = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier)
        .filter(|declaration| local_declaration_owner(index, *declaration) == Some(owner))
        .position(|declaration| declaration == key.node)?;
    Some(
        u32::try_from(slot)
            .map(|index| LocalSlot {
                owner: AstNodeKey { node: owner, ..key },
                index,
            })
            .map_err(|_| SemanticError::unavailable("local_slot")),
    )
}

fn local_declaration_owner(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let parent = parent_node(index, declaration)?;
    if !matches!(
        index.kind(parent)?,
        beskid_analysis::syntax_query::NodeKind::Parameter
            | beskid_analysis::syntax_query::NodeKind::LetStatement
            | beskid_analysis::syntax_query::NodeKind::LambdaParameter
            | beskid_analysis::syntax_query::NodeKind::ForStatement
            | beskid_analysis::syntax_query::NodeKind::Pattern
    ) {
        return None;
    }
    nearest_ancestor(index, parent, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                | beskid_analysis::syntax_query::NodeKind::TestDefinition
                | beskid_analysis::syntax_query::NodeKind::LambdaExpression
        )
    })
}

fn local_declaration_is_mutable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let Some(parent) = parent_node(index, declaration) else {
        return false;
    };
    let Some(node) = index.node_at(program, parent) else {
        return false;
    };
    node.of::<beskid_analysis::syntax::LetStatement>()
        .is_some_and(|binding| binding.mutable)
        || node
            .of::<beskid_analysis::syntax::Parameter>()
            .is_some_and(|parameter| parameter.mutable)
}

fn resolve_lexical_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut best: Option<(usize, u32, beskid_analysis::syntax::AstNodeId)> = None;
    for declaration in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier) {
        let Some(identifier) = index
            .node_at(program, declaration)
            .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        else {
            continue;
        };
        if identifier.name != name {
            continue;
        }
        let Some(scope) = local_declaration_scope(index, declaration, reference) else {
            continue;
        };
        let Some(distance) = ancestor_distance(index, scope, reference) else {
            continue;
        };
        let rank = (distance, u32::MAX - declaration.0, declaration);
        if best.is_none_or(|current| rank < current) {
            best = Some(rank);
        }
    }
    best.map(|(_, _, declaration)| declaration)
}

fn local_declaration_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
    reference: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            if declaration.0 >= reference.0 || is_ancestor(index, parent, reference) {
                return None;
            }
            nearest_ancestor(index, parent, |kind| {
                matches!(
                    kind,
                    beskid_analysis::syntax_query::NodeKind::Block
                        | beskid_analysis::syntax_query::NodeKind::TestDefinition
                )
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::Parameter => {
            nearest_ancestor(index, parent, |kind| {
                matches!(
                    kind,
                    beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                        | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                )
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => {
            nearest_ancestor(index, parent, |kind| {
                kind == beskid_analysis::syntax_query::NodeKind::LambdaExpression
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .children(parent)?
            .iter()
            .copied()
            .find(|child| {
                index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Block)
            })
            .filter(|scope| is_ancestor(index, *scope, reference)),
        beskid_analysis::syntax_query::NodeKind::Pattern => {
            nearest_ancestor(index, parent, |kind| {
                kind == beskid_analysis::syntax_query::NodeKind::MatchArm
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        _ => None,
    }
}

fn parent_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    index.metadata().get(node.0 as usize)?.parent
}

fn nearest_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
    predicate: impl Fn(beskid_analysis::syntax_query::NodeKind) -> bool,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if index.kind(candidate).is_some_and(&predicate) {
            return Some(candidate);
        }
        current = parent_node(index, candidate);
    }
    None
}

fn is_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    ancestor_distance(index, ancestor, node).is_some()
}

fn ancestor_distance(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<usize> {
    let mut current = Some(node);
    let mut distance = 0usize;
    while let Some(candidate) = current {
        if candidate == ancestor {
            return Some(distance);
        }
        current = parent_node(index, candidate);
        distance += 1;
    }
    None
}

#[salsa::tracked]
fn node_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        semantic_type_for_node(program, index, key.node, node)
    })?
    .transpose()
}

fn semantic_type_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
        return Some(Ok(semantic_type_for_literal(literal)));
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
        return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        return Some(semantic_type_for_local_path(
            program,
            index,
            reference,
            &path.path.node,
        ));
    }
    if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
        return Some(semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.left.node,
            binary.op.node,
            &binary.right.node,
        ));
    }
    if let Some(match_expression) = node.of::<beskid_analysis::syntax::MatchExpression>() {
        let mut result = None;
        for arm in &match_expression.arms {
            let arm_type =
                match semantic_type_for_expression(program, index, reference, &arm.node.value.node)
                {
                    Ok(arm_type) => arm_type,
                    Err(error) => return Some(Err(error)),
                };
            if result
                .replace(arm_type)
                .is_some_and(|previous| previous != arm_type)
            {
                return Some(Err(SemanticError::unavailable("node_type")));
            }
        }
        return result
            .map(Ok)
            .or_else(|| Some(Err(SemanticError::unavailable("node_type"))));
    }
    if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
        return Some(semantic_type_for_expression(
            program, index, reference, expression,
        ));
    }
    if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
        return Some(semantic_type_from_syntax(syntax_type));
    }
    if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
        return local_declaration_type(program, index, reference);
    }
    expression_fact_target(node.node_kind()).then(|| Err(SemanticError::unavailable("node_type")))
}

fn semantic_type_for_expression(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    match expression {
        beskid_analysis::syntax::Expression::Literal(literal) => {
            Ok(semantic_type_for_literal(&literal.node.literal.node))
        }
        beskid_analysis::syntax::Expression::Path(path) => {
            semantic_type_for_local_path(program, index, reference, &path.node.path.node)
        }
        beskid_analysis::syntax::Expression::Grouped(grouped) => {
            semantic_type_for_expression(program, index, reference, &grouped.node.expr.node)
        }
        beskid_analysis::syntax::Expression::Binary(binary) => semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.node.left.node,
            binary.node.op.node,
            &binary.node.right.node,
        ),
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

fn semantic_type_for_binary_operands(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    left: &beskid_analysis::syntax::Expression,
    op: beskid_analysis::syntax::BinaryOp,
    right: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = semantic_type_for_expression(program, index, reference, left)?;
    let right = semantic_type_for_expression(program, index, reference, right)?;
    use beskid_analysis::syntax::BinaryOp;
    match op {
        BinaryOp::Or | BinaryOp::And if left == SemanticTypeId::BOOL && right == SemanticTypeId::BOOL => {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte
            if left == right =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add if left == SemanticTypeId::STRING && right == SemanticTypeId::STRING => {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left == right && primitive_numeric(left) =>
        {
            Ok(left)
        }
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

fn semantic_type_for_local_path(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    let [segment] = path.segments.as_slice() else {
        return Err(SemanticError::unavailable("node_type"));
    };
    if !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("node_type"));
    }
    let declaration = resolve_lexical_declaration(
        program,
        index,
        reference,
        segment.node.name.node.name.as_str(),
    )
    .ok_or_else(|| SemanticError::unavailable("node_type"))?;
    local_declaration_type(program, index, declaration)
        .unwrap_or_else(|| Err(SemanticError::unavailable("node_type")))
}

fn local_declaration_type(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| semantic_type_from_syntax(&parameter.ty.node)),
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LambdaParameter>()
            .map(|parameter| {
                parameter.ty.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("node_type")),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            }),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .map(|statement| {
                statement.type_annotation.as_ref().map_or_else(
                    || semantic_type_for_expression(program, index, parent, &statement.value.node),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            }),
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::ForStatement>()
            .map(|statement| {
                element_type_for_for_iterable(program, index, parent, &statement.iterable.node)
            }),
        _ => None,
    }
}

fn element_type_for_for_iterable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    for_statement: beskid_analysis::syntax::AstNodeId,
    iterable: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let beskid_analysis::syntax::Expression::Call(call) = iterable else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let beskid_analysis::syntax::Expression::Path(path) = &call.node.callee.node else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    }
    let [start, _end] = call.node.args.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    semantic_type_for_expression(program, index, for_statement, &start.node)
}

fn semantic_type_for_literal(literal: &beskid_analysis::syntax::Literal) -> SemanticTypeId {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i32") => {
            SemanticTypeId::I32
        }
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i64") => {
            SemanticTypeId::I64
        }
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_u8") => {
            SemanticTypeId::U8
        }
        beskid_analysis::syntax::Literal::Integer(_) => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Float(_) => SemanticTypeId::F64,
        beskid_analysis::syntax::Literal::String(_) => SemanticTypeId::STRING,
        beskid_analysis::syntax::Literal::Char(_) => SemanticTypeId::CHAR,
        beskid_analysis::syntax::Literal::Bool(_) => SemanticTypeId::BOOL,
    }
}

#[salsa::tracked]
fn call_lowering_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CallLowering> {
    with_node(db, syntax, key, |program, index, node| {
        call_lowering_for_node(db, program, index, key, node)
    })?
    .transpose()
}

#[salsa::tracked]
fn call_arguments_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let mut arguments = Vec::with_capacity(call.args.len() + 1);
        if let beskid_analysis::syntax::Expression::Member(member) = &call.callee.node {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            let callee = normalized_expression_node(index, callee);
            let Some(receiver) = index.direct_child_id(
                program,
                callee,
                beskid_analysis::syntax_query::DynNodeRef::from(member.node.target.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            arguments.push(AstNodeKey {
                node: normalized_expression_node(index, receiver),
                ..key
            });
        } else if let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node
            && nominal_local_member_receiver(db, program, index, key, &path.node.path.node)
                .is_some()
        {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            arguments.push(AstNodeKey {
                node: normalized_expression_node(index, callee),
                ..key
            });
        }
        let explicit = match call
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(
                        program,
                        key.node,
                        beskid_analysis::syntax_query::DynNodeRef::from(argument),
                    )
                    .map(|node| AstNodeKey { node, ..key })
                    .ok_or_else(|| SemanticError::unavailable("call_arguments"))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(explicit) => explicit,
            Err(error) => return Some(Err(error)),
        };
        arguments.extend(explicit);
        Some(Ok(arguments.into()))
    })?
    .transpose()
}

#[salsa::tracked]
fn range_for_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RangeForFact> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
            return None;
        }
        let [start, end] = call.args.as_slice() else {
            return Some(Err(SemanticError::unavailable("range_for_fact")));
        };
        let start = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(start),
        )?;
        let end = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(end),
        )?;
        Some(Ok(RangeForFact {
            start: AstNodeKey { node: normalized_expression_node(index, start), ..key },
            end: AstNodeKey { node: normalized_expression_node(index, end), ..key },
        }))
    })?
    .transpose()
}

#[salsa::tracked]
fn for_iterator_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ForIteratorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let statement = node.of::<beskid_analysis::syntax::ForStatement>()?;
        let declaration = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(&statement.iterator),
        )?;
        match element_type_for_for_iterable(program, index, key.node, &statement.iterable.node) {
            Ok(element_type) => Some(Ok(ForIteratorFact {
                declaration: AstNodeKey {
                    node: declaration,
                    ..key
                },
                element_type,
            })),
            Err(error) => Some(Err(error)),
        }
    })?
    .transpose()
}

fn call_lowering_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CallLowering, SemanticError>> {
    let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
    Some(match &call.callee.node {
        expression if expression_is_lambda(expression) => Ok(CallLowering::Dynamic),
        beskid_analysis::syntax::Expression::Path(path) => {
            let path = &path.node.path.node;
            if imported_generic_nominal_receiver_requires_instantiation(db, key, path) {
                Err(SemanticError::unavailable("generic_receiver_instantiation"))
            } else if let Some(service) = corelib_service_for(db, key, path) {
                Ok(CallLowering::CorelibService(service))
            } else if let Some((declaration, _)) =
                nominal_local_member_receiver(db, program, index, key, path)
            {
                Ok(CallLowering::Direct(declaration))
            } else if path
                .segments
                .iter()
                .any(|segment| !segment.node.type_args.is_empty())
            {
                if let Some(instantiation) =
                    generic_call_instantiation_for_node(db, program, index, key, path)
                {
                    Ok(CallLowering::Direct(instantiation.declaration))
                } else if let Some(declaration) =
                    resolve_item_declaration_candidate(db, program, index, key, path)
                    && generic_call_uses_parameter_type_arguments(db, key, declaration, path)
                {
                    Ok(CallLowering::Direct(declaration))
                } else if imported_call_receiver_exists(db, key, path) {
                    Ok(CallLowering::Dynamic)
                } else {
                    Err(SemanticError::unavailable("generic_call_instantiation"))
                }
            } else if let Some(declaration) =
                resolve_item_declaration(db, program, index, key, path)
            {
                if function_declares_generics(db, declaration) && call.args.is_empty() {
                    Err(SemanticError::unavailable("generic_call_instantiation"))
                } else {
                    Ok(CallLowering::Direct(declaration))
                }
            } else if imported_call_receiver_exists(db, key, path)
                || (path
                    .segments
                    .iter()
                    .all(|segment| segment.node.type_args.is_empty())
                    && beskid_analysis::builtins::builtin_for_path(
                        &path
                            .segments
                            .iter()
                            .map(|segment| segment.node.name.node.name.clone())
                            .collect::<Vec<_>>(),
                    )
                    .is_some())
            {
                Ok(CallLowering::Dynamic)
            } else if let Some(declaration) =
                resolve_local_extern_contract_method(program, index, key, path)
            {
                Ok(CallLowering::Direct(declaration))
            } else {
                Err(SemanticError::unavailable("call_lowering"))
            }
        }
        beskid_analysis::syntax::Expression::Member(member) => {
            // Nominal methods lower Direct when declaration authority exists.
            // Extern/contract members and other receivers without a syntax method
            // declaration remain Dynamic rather than unavailable, matching Path
            // import/builtin fallback so production JIT/AOT can emit the call.
            Ok(
                method_declaration_for_member_receiver(db, program, index, key, call, member)
                    .map(CallLowering::Direct)
                    .unwrap_or(CallLowering::Dynamic),
            )
        }
        _ => Err(SemanticError::unavailable("call_lowering")),
    })
}

/// Resolve `Contract.method` when `Contract` is an `[Extern]` contract in the current unit.
///
/// Qualified paths parse as Path (not Member), so extern FFI calls like `C.getpid()` need this
/// authority before call_lowering fails closed as unavailable.
fn resolve_local_extern_contract_method(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let [contract_segment, method_segment] = path.segments.as_slice() else {
        return None;
    };
    if !contract_segment.node.type_args.is_empty() || !method_segment.node.type_args.is_empty() {
        return None;
    }
    let contract_name = contract_segment.node.name.node.name.as_str();
    let method_name = method_segment.node.name.node.name.as_str();
    let contracts = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::ContractDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::ContractDefinition>())
                .is_some_and(|contract| {
                    contract.name.node.name == contract_name
                        && contract.attributes.iter().any(|attribute| {
                            attribute.node.name.node.name == "Extern"
                        })
                })
        })
        .collect::<Vec<_>>();
    let [contract_id] = contracts.as_slice() else {
        return None;
    };
    let methods = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::ContractMethodSignature)
        .filter(|candidate| {
            let Some(metadata) = index.metadata_for(key.generation, *candidate) else {
                return false;
            };
            let mut parent = metadata.parent;
            let mut under_contract = false;
            while let Some(parent_id) = parent {
                if parent_id == *contract_id {
                    under_contract = true;
                    break;
                }
                parent = index
                    .metadata_for(key.generation, parent_id)
                    .and_then(|node| node.parent);
            }
            under_contract
                && index
                    .node_at(program, *candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::ContractMethodSignature>())
                    .is_some_and(|method| method.name.node.name == method_name)
        })
        .collect::<Vec<_>>();
    let [method_id] = methods.as_slice() else {
        return None;
    };
    Some(AstNodeKey {
        unit: key.unit,
        generation: key.generation,
        node: *method_id,
    })
}


/// Return Extern import metadata for a [`ContractMethodSignature`] declaration key.
pub fn extern_contract_import_for_declaration(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(String, Option<String>, Option<String>)> {
    let syntax = db.syntax_unit(declaration.unit)?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let method = index
        .node_at(program, declaration.node)?
        .of::<beskid_analysis::syntax::ContractMethodSignature>()?;
    let mut parent = index
        .metadata_for(declaration.generation, declaration.node)?
        .parent;
    let mut contract = None;
    while let Some(parent_id) = parent {
        if let Some(definition) = index
            .node_at(program, parent_id)
            .and_then(|node| node.of::<beskid_analysis::syntax::ContractDefinition>())
        {
            contract = Some(definition);
            break;
        }
        parent = index
            .metadata_for(declaration.generation, parent_id)
            .and_then(|node| node.parent);
    }
    let contract = contract?;
    let extern_attr = contract
        .attributes
        .iter()
        .find(|attribute| attribute.node.name.node.name == "Extern")?;
    let mut abi = None;
    let mut library = None;
    for argument in &extern_attr.node.arguments {
        let value = match &argument.node.value.node {
            beskid_analysis::syntax::Expression::Literal(literal) => {
                match &literal.node.literal.node {
                    beskid_analysis::syntax::Literal::String(raw) => raw
                        .strip_prefix('"')
                        .and_then(|value| value.strip_suffix('"'))
                        .map(str::to_owned),
                    _ => None,
                }
            }
            _ => None,
        };
        match argument.node.name.node.name.as_str() {
            "Abi" => abi = value,
            "Library" => library = value,
            _ => {}
        }
    }
    Some((method.name.node.name.clone(), abi, library))
}

/// Resolve one syntax-authorized nominal receiver and its uniquely declared method. Struct
/// literals and unqualified locals with explicit nominal parameter or let annotations provide
/// the required declaration authority. Inferred locals, extensions, overloads, and chained
/// receivers remain unavailable rather than reconstructing retired HIR type information.
fn method_declaration_for_member_receiver(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    call: &beskid_analysis::syntax::CallExpression,
    member: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::MemberExpression>,
) -> Option<AstNodeKey> {
    let callee = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
    )?;
    let callee = normalized_expression_node(index, callee);
    let receiver = index.direct_child_id(
        program,
        callee,
        beskid_analysis::syntax_query::DynNodeRef::from(member.node.target.as_ref()),
    )?;
    let receiver = AstNodeKey {
        node: normalized_expression_node(index, receiver),
        ..key
    };
    let declaration = aggregate_literal_declaration(db, receiver)
        .ok()
        .flatten()
        .or_else(|| {
            let receiver_node = index.node_at(program, receiver.node)?;
            let path = receiver_node.of::<beskid_analysis::syntax::PathExpression>()?;
            let [segment] = path.path.node.segments.as_slice() else {
                return None;
            };
            if !segment.node.type_args.is_empty() {
                return None;
            }
            nominal_local_receiver_declaration(
                db,
                program,
                index,
                key,
                segment.node.name.node.name.as_str(),
            )
            .map(|(declaration, _)| declaration)
        })?;
    unique_nominal_method_declaration(db, declaration, &member.node.member.node.name)
}

/// Resolve an ordinary `local.Method()` spelling only when `local` resolves to an explicitly
/// annotated nominal parameter or let. Qualified static paths and inferred locals are left to
/// their existing path rules or remain unavailable.
fn nominal_local_member_receiver(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let [receiver, member] = path.segments.as_slice() else {
        return None;
    };
    if !receiver.node.type_args.is_empty() || !member.node.type_args.is_empty() {
        return None;
    }
    let (declaration, receiver) = nominal_local_receiver_declaration(
        db,
        program,
        index,
        key,
        receiver.node.name.node.name.as_str(),
    )?;
    unique_nominal_method_declaration(db, declaration, &member.node.name.node.name)
        .map(|method| (method, receiver))
}

#[salsa::tracked]
fn nominal_member_receiver_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        nominal_local_member_receiver(db, program, index, key, &path.path.node)
            .map(|(_, receiver)| Ok(receiver))
    })?
    .transpose()
}

fn unique_nominal_method_declaration(
    db: &dyn Db,
    declaration: AstNodeKey,
    method_name: &str,
) -> Option<AstNodeKey> {
    let declaration_syntax = db.syntax_unit(declaration.unit)?;
    let declaration_program = declaration_syntax.expanded_program(db);
    let declaration_index = declaration_syntax.syntax_index(db);
    declaration_index
        .node_at(declaration_program, declaration.node)?
        .of::<beskid_analysis::syntax::TypeDefinition>()?;
    let methods = declaration_index
        .children(declaration.node)?
        .iter()
        .copied()
        .filter(|candidate| {
            declaration_index
                .node_at(declaration_program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
                .is_some_and(|method| method.name.node.name == method_name)
        })
        .map(|node| AstNodeKey {
            unit: declaration.unit,
            generation: declaration.generation,
            node,
        })
        .collect::<Vec<_>>();
    (methods.len() == 1).then(|| methods[0])
}

/// Resolve a legacy syscall spelling only when its current source unit was admitted by the
/// compiler-minted Corelib service constructor. The same builtins remain dynamic everywhere else.
fn corelib_service_for(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<CorelibService> {
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .corelib_services
        .get(&(key.unit, key.generation))?
        .iter()
        .copied()
        .find(|service| service.name == name)
}

#[salsa::tracked]
fn generic_call_instantiation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallInstantiation> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        generic_call_instantiation_for_node(db, program, index, key, &path.node.path.node).map(Ok)
    })?
    .transpose()
}

#[salsa::tracked]
fn generic_call_specialization_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallSpecialization> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        let lowering = match call_lowering(db, key) {
            Ok(Some(lowering)) => lowering,
            Ok(None) => return None,
            Err(error) => return Some(Err(error)),
        };
        let declaration = match lowering {
            CallLowering::Direct(declaration) => declaration,
            CallLowering::Dynamic | CallLowering::Runtime(_) | CallLowering::CorelibService(_) => {
                return None;
            }
        };
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let declaration_node = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?;
        let function = declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        if function.generics.is_empty() {
            return None;
        }
        let signature = call_abi_signature(db, key).ok().flatten()?;
        Some(Ok(GenericCallSpecialization {
            declaration,
            signature,
        }))
    })?
    .transpose()
}

fn explicit_generic_type_argument_syntax(
    path: &beskid_analysis::syntax::Path,
) -> Option<&[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>]> {
    let terminal = path.segments.last()?;
    let receiver = path.segments.get(..path.segments.len().checked_sub(1)?)?;
    let receiver_with_arguments = receiver
        .iter()
        .filter(|segment| !segment.node.type_args.is_empty())
        .collect::<Vec<_>>();
    let terminal_has_arguments = !terminal.node.type_args.is_empty();
    match (terminal_has_arguments, receiver_with_arguments.as_slice()) {
        (true, []) => Some(terminal.node.type_args.as_slice()),
        (false, [receiver]) => Some(receiver.node.type_args.as_slice()),
        _ => None,
    }
}

fn type_syntax_is_generic_parameter_reference(
    syntax_type: &beskid_analysis::syntax::Type,
    parameter_name: &str,
) -> bool {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return false;
    };
    let [segment] = path.node.segments.as_slice() else {
        return false;
    };
    segment.node.type_args.is_empty() && segment.node.name.node.name == parameter_name
}

fn generic_call_uses_parameter_type_arguments(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some(type_arguments) = explicit_generic_type_argument_syntax(path) else {
        return false;
    };
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, declaration) {
        return false;
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return false;
    };
    if function.generics.len() != type_arguments.len() {
        return false;
    }
    type_arguments
        .iter()
        .zip(function.generics.iter())
        .all(|(argument, generic)| {
            abi_type_from_syntax(db, key, &argument.node).is_ok()
                || type_syntax_is_generic_parameter_reference(
                    &argument.node,
                    generic.node.name.as_str(),
                )
        })
}

/// A two-segment imported nominal call can spell either a module member or a static member on a
/// generic nominal type.  The latter has no concrete receiver ABI until the source supplies the
/// receiver arguments (`Hub<i64>.Create()`), so it must not be treated as the imported module's
/// direct function.  Terminal method arguments remain independently valid (`Hub.Create<i64>()`).
fn imported_generic_nominal_receiver_requires_instantiation(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let [receiver, method] = path.segments.as_slice() else {
        return false;
    };
    if !receiver.node.type_args.is_empty() || !method.node.type_args.is_empty() {
        return false;
    }
    let receiver_name = receiver.node.name.node.name.as_str();
    let targets = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .into_iter()
        .flatten()
        .filter(|import| import.binding == receiver_name)
        .map(|import| import.target)
        .collect::<Vec<_>>();
    let [target] = targets.as_slice() else {
        return false;
    };
    exported_generic_type_named(db, *target, key.generation, receiver_name)
}

fn exported_generic_type_named(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> bool {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let Some(syntax) = db.syntax_unit(current) else {
            continue;
        };
        if syntax.generation(db) != generation {
            continue;
        }
        if syntax
            .syntax_index(db)
            .ids_of_kind(beskid_analysis::syntax_query::NodeKind::TypeDefinition)
            .any(|candidate| {
                syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                    .is_some_and(|definition| {
                        definition.name.node.name == name && !definition.generics.is_empty()
                    })
            })
        {
            return true;
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    false
}

fn generic_call_instantiation_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<GenericCallInstantiation> {
    let argument_syntax = explicit_generic_type_argument_syntax(path)?;
    let argument_count = u8::try_from(argument_syntax.len()).ok()?;
    (argument_count > 0).then_some(())?;
    let declaration = resolve_item_declaration_candidate(db, program, index, key, path)?;
    let syntax = db.syntax_unit(declaration.unit)?;
    syntax.accepts_key(db, declaration).then_some(())?;
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<beskid_analysis::syntax::FunctionDefinition>()?;
    (function.generics.len() == usize::from(argument_count)).then_some(())?;
    let mut concrete_arguments = Vec::with_capacity(argument_syntax.len());
    for (argument, generic) in argument_syntax.iter().zip(function.generics.iter()) {
        match abi_type_from_syntax(db, key, &argument.node) {
            Ok(concrete) => concrete_arguments.push(concrete),
            Err(_)
                if type_syntax_is_generic_parameter_reference(
                    &argument.node,
                    generic.node.name.as_str(),
                ) => {}
            Err(_) => return None,
        }
    }
    Some(GenericCallInstantiation {
        declaration,
        argument_count,
        arguments: concrete_arguments.into(),
    })
}

fn function_declares_generics(db: &dyn Db, declaration: AstNodeKey) -> bool {
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, declaration) {
        return false;
    }
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| !function.generics.is_empty())
}

/// Whether a qualified call's receiver is an exact current import target.
/// Imported type/module member calls have no direct item edge; unknown qualified calls remain
/// unavailable instead of being guessed.
fn imported_call_receiver_exists(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some((_member, receiver)) = path.segments.split_last() else {
        return false;
    };
    if receiver.is_empty() {
        return false;
    }
    let receiver = receiver
        .iter()
        .map(|segment| segment.node.name.node.name.as_str())
        .collect::<Vec<_>>();
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .is_some_and(|imports| {
            imports
                .iter()
                .filter(|import| {
                    (receiver.len() == 1 && import.binding == receiver[0])
                        || (import.path.len() >= receiver.len()
                            && import.path[import.path.len() - receiver.len()..]
                                .iter()
                                .map(String::as_str)
                                .eq(receiver.iter().copied()))
                })
                .take(2)
                .count()
                == 1
        })
}

fn expression_is_lambda(expression: &beskid_analysis::syntax::Expression) -> bool {
    match expression {
        beskid_analysis::syntax::Expression::Lambda(_) => true,
        beskid_analysis::syntax::Expression::Grouped(grouped) => {
            expression_is_lambda(&grouped.node.expr.node)
        }
        _ => false,
    }
}

#[salsa::tracked]
fn cast_intents_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[CastIntent]>> {
    with_node(db, syntax, key, |program, index, node| {
        cast_intents_for_node(db, program, index, key, node)
    })?
    .transpose()
}

fn cast_intents_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<Arc<[CastIntent]>, SemanticError>> {
    if !expression_fact_target(node.node_kind()) {
        return None;
    }
    let actual = match semantic_type_for_node(program, index, key.node, node)? {
        Ok(actual) => actual,
        Err(_) => return Some(Err(SemanticError::unavailable("cast_intents"))),
    };
    let expected = match expected_cast_type(db, program, index, key)? {
        Ok(expected) => expected,
        Err(error) => return Some(Err(error)),
    };
    if actual == expected {
        return Some(Ok(Arc::from([])));
    }
    if primitive_numeric(actual) && primitive_numeric(expected) {
        return Some(Ok(Arc::from([CastIntent {
            from: actual,
            to: expected,
        }])));
    }
    Some(Err(SemanticError::unavailable("cast_intents")))
}

/// Resolve the exact explicit constraint that gives an expression a numeric coercion target.
///
/// A typed `let` remains the original source of cast intent.  A direct call contributes the
/// corresponding parameter type only when its declaration or canonical ABI-v5 intrinsic
/// signature is known from generation-safe syntax facts.  This establishes the target before
/// ISLE emits the literal; lowering never guesses a machine-width conversion.
fn expected_cast_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    if let Some(binary_id) = nearest_ancestor(index, key.node, |kind| {
        kind == beskid_analysis::syntax_query::NodeKind::BinaryExpression
    }) {
        let operands = index
            .children(binary_id)?
            .iter()
            .copied()
            .filter(|child| {
                index.kind(*child) != Some(beskid_analysis::syntax_query::NodeKind::BinaryOp)
            })
            .collect::<Vec<_>>();
        let operand = operands
            .iter()
            .copied()
            .find(|operand| is_ancestor(index, *operand, key.node))?;
        let sibling = operands
            .into_iter()
            .find(|candidate| *candidate != operand)?;
        if is_transparent_binary_operand_path(index, operand, key.node) {
            let sibling_node = index.node_at(program, sibling)?;
            return semantic_type_for_node(program, index, sibling, sibling_node)
                .map(|result| result.map_err(|_| SemanticError::unavailable("cast_intents")));
        }
    }

    if let Some(statement_id) = nearest_ancestor(index, key.node, |kind| {
        kind == beskid_analysis::syntax_query::NodeKind::LetStatement
    }) {
        let statement = index
            .node_at(program, statement_id)?
            .of::<beskid_analysis::syntax::LetStatement>()?;
        let value_id = index
            .children(statement_id)?
            .iter()
            .copied()
            .find(|child| {
                index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Expression)
            })?;
        if !is_ancestor(index, value_id, key.node) {
            return None;
        }
        return Some(
            statement
                .type_annotation
                .as_ref()
                .ok_or_else(|| SemanticError::unavailable("cast_intents"))
                .and_then(|expected| semantic_type_from_syntax(&expected.node)),
        );
    }

    let call_id = nearest_ancestor(index, key.node, |kind| {
        kind == beskid_analysis::syntax_query::NodeKind::CallExpression
    })?;
    let call = index
        .node_at(program, call_id)?
        .of::<beskid_analysis::syntax::CallExpression>()?;
    let argument_index = call.args.iter().position(|argument| {
        index
            .direct_child_id(
                program,
                call_id,
                beskid_analysis::syntax_query::DynNodeRef::from(argument),
            )
            .is_some_and(|argument_id| is_ancestor(index, argument_id, key.node))
    })?;
    let expected = match &call.callee.node {
        beskid_analysis::syntax::Expression::Path(path) => {
            let path = &path.node.path.node;
            if let Some(declaration) = resolve_item_declaration(db, program, index, key, path) {
                item_signature(db, declaration)
                    .ok()
                    .flatten()
                    .and_then(|signature| signature.parameters.get(argument_index).copied())
            } else if path.segments.len() == 1
                && resolve_lexical_declaration(
                    program,
                    index,
                    call_id,
                    path.segments[0].node.name.node.name.as_str(),
                )
                .is_none()
            {
                canonical_intrinsic_parameter_type(
                    path.segments[0].node.name.node.name.as_str(),
                    argument_index,
                )
            } else {
                None
            }
        }
        _ => None,
    };
    Some(expected.ok_or_else(|| SemanticError::unavailable("cast_intents")))
}

/// A binary comparison constrains only its own operand expression, not a nested call argument.
/// For example, `object == NativePointer(0)` must retain `NativePointer`'s `word` parameter
/// intent for `0`; the outer pointer comparison has no authority to coerce that argument.
fn is_transparent_binary_operand_path(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    operand: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    let mut current = node;
    while current != operand {
        let Some(parent) = parent_node(index, current) else {
            return false;
        };
        if !matches!(
            index.kind(parent),
            Some(NodeKind::Expression | NodeKind::LiteralExpression | NodeKind::GroupedExpression)
        ) {
            return false;
        }
        current = parent;
    }
    true
}

/// ABI-v5 intrinsic signatures are target-independent.  Selecting a supported target merely
/// accesses the generated canonical manifest; codegen still requires its non-forgeable runtime
/// capability before it can import any of these symbols.
fn canonical_intrinsic_parameter_type(name: &str, argument_index: usize) -> Option<SemanticTypeId> {
    let target = TargetMetadata::supported().into_iter().next()?;
    let manifest = AbiManifestV5::canonical_runtime(target);
    let intrinsic = manifest.intrinsic_metadata(name)?;
    abi_semantic_type(*intrinsic.params.get(argument_index)?)
}

fn abi_semantic_type(ty: AbiType) -> Option<SemanticTypeId> {
    Some(match ty {
        AbiType::Void => return None,
        AbiType::Pointer => SemanticTypeId::POINTER,
        AbiType::USize => SemanticTypeId::WORD,
        AbiType::I8 | AbiType::U8 => SemanticTypeId::U8,
        AbiType::I32 => SemanticTypeId::I32,
        AbiType::I64 => SemanticTypeId::I64,
        AbiType::F64 => SemanticTypeId::F64,
        _ => return None,
    })
}

fn primitive_numeric(semantic_type: SemanticTypeId) -> bool {
    matches!(
        semantic_type,
        SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U8
            | SemanticTypeId::WORD
            | SemanticTypeId::F64
    )
}

fn expression_fact_target(kind: beskid_analysis::syntax_query::NodeKind) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    matches!(
        kind,
        NodeKind::Expression
            | NodeKind::AssignExpression
            | NodeKind::BinaryExpression
            | NodeKind::UnaryExpression
            | NodeKind::CallExpression
            | NodeKind::MemberExpression
            | NodeKind::LiteralExpression
            | NodeKind::PathExpression
            | NodeKind::StructLiteralExpression
            | NodeKind::IndexExpression
            | NodeKind::ArrayLiteralExpression
            | NodeKind::CodeStringLiteral
            | NodeKind::EnumConstructorExpression
            | NodeKind::BlockExpression
            | NodeKind::GroupedExpression
            | NodeKind::TryExpression
            | NodeKind::SpawnExpression
            | NodeKind::LambdaExpression
            | NodeKind::MatchExpression
            | NodeKind::Literal
    )
}

#[salsa::tracked]
fn control_flow_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ControlFlow> {
    with_node(db, syntax, key, |_program, _index, node| {
        control_flow_for_node(node).map(|may_fall_through| ControlFlow { may_fall_through })
    })
}

fn control_flow_for_node(node: beskid_analysis::syntax_query::DynNodeRef<'_>) -> Option<bool> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(block_may_fall_through(&function.body.node));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(block_may_fall_through(&method.body.node));
    }
    if let Some(test) = node.of::<beskid_analysis::syntax::TestDefinition>() {
        return Some(statements_may_fall_through(&test.statements));
    }
    if let Some(block) = node.of::<beskid_analysis::syntax::Block>() {
        return Some(block_may_fall_through(block));
    }
    if let Some(statement) = node.of::<beskid_analysis::syntax::Statement>() {
        return Some(statement_may_fall_through(statement));
    }
    if let Some(if_statement) = node.of::<beskid_analysis::syntax::IfStatement>() {
        return Some(if_may_fall_through(if_statement));
    }
    if let Some(with_statement) = node.of::<beskid_analysis::syntax::WithStatement>() {
        return Some(block_may_fall_through(&with_statement.body.node));
    }
    if node
        .of::<beskid_analysis::syntax::ReturnStatement>()
        .is_some()
        || node
            .of::<beskid_analysis::syntax::BreakStatement>()
            .is_some()
        || node
            .of::<beskid_analysis::syntax::ContinueStatement>()
            .is_some()
    {
        return Some(false);
    }
    None
}

fn block_may_fall_through(block: &beskid_analysis::syntax::Block) -> bool {
    statements_may_fall_through(&block.statements)
}

fn statements_may_fall_through(
    statements: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Statement>],
) -> bool {
    statements
        .iter()
        .all(|statement| statement_may_fall_through(&statement.node))
}

fn statement_may_fall_through(statement: &beskid_analysis::syntax::Statement) -> bool {
    match statement {
        beskid_analysis::syntax::Statement::Return(_)
        | beskid_analysis::syntax::Statement::Break(_)
        | beskid_analysis::syntax::Statement::Continue(_) => false,
        beskid_analysis::syntax::Statement::If(if_statement) => {
            if_may_fall_through(&if_statement.node)
        }
        beskid_analysis::syntax::Statement::With(with_statement) => {
            block_may_fall_through(&with_statement.node.body.node)
        }
        beskid_analysis::syntax::Statement::Let(_)
        | beskid_analysis::syntax::Statement::While(_)
        | beskid_analysis::syntax::Statement::For(_)
        | beskid_analysis::syntax::Statement::Launch(_)
        | beskid_analysis::syntax::Statement::Expression(_) => true,
    }
}

fn if_may_fall_through(if_statement: &beskid_analysis::syntax::IfStatement) -> bool {
    if block_may_fall_through(&if_statement.then_block.node) {
        return true;
    }
    let Some(else_branch) = &if_statement.else_branch else {
        return true;
    };
    match &else_branch.node {
        beskid_analysis::syntax::ElseBranch::If(nested) => if_may_fall_through(&nested.node),
        beskid_analysis::syntax::ElseBranch::Block(block) => block_may_fall_through(&block.node),
    }
}

#[salsa::tracked]
fn item_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| {
        item_signature_for_node(node)
    })?
    .transpose()
}

fn item_signature_for_node(
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ItemSignature, SemanticError>> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(signature_from_syntax(
            &function.parameters,
            function.return_type.as_ref(),
        ));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(signature_from_syntax(
            &method.parameters,
            method.return_type.as_ref(),
        ));
    }
    if node
        .of::<beskid_analysis::syntax::TestDefinition>()
        .is_some()
    {
        return Some(Ok(ItemSignature {
            parameters: Arc::from([]),
            result: SemanticTypeId::UNIT,
        }));
    }
    if let Some(contract) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        return Some(signature_from_syntax(
            &contract.parameters,
            contract.return_type.as_ref(),
        ));
    }
    None
}

fn signature_from_syntax(
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| semantic_type_from_syntax(&parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        semantic_type_from_syntax(&return_type.node)
    })?;
    Ok(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

/// ABI-representation signature for syntax-only lowering.
///
/// Nominal source identity remains in [`item_signature`]. ABI v5 passes every declared nominal
/// aggregate by reference, represented as one target-sized pointer; only source declaration
/// resolution is needed to prove that representation.
#[salsa::tracked]
fn item_abi_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            // Generic declarations have no single item ABI. Call sites must prove a concrete
            // specialization; otherwise module emission would register `Item` while calls import
            // `SpecializedItem` (for example `Channel<T> Create<T>()` collapsing to POINTER).
            if !function.generics.is_empty() {
                return None;
            }
            return Some(abi_signature_from_syntax(
                db,
                key,
                &function.parameters,
                function.return_type.as_ref(),
            ));
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            let mut signature = match abi_signature_from_syntax(
                db,
                key,
                &method.parameters,
                method.return_type.as_ref(),
            ) {
                Ok(signature) => signature,
                Err(error) => return Some(Err(error)),
            };
            let mut parameters = Vec::with_capacity(signature.parameters.len() + 1);
            parameters.push(SemanticTypeId::POINTER);
            parameters.extend(signature.parameters.iter().copied());
            signature.parameters = parameters.into();
            return Some(Ok(signature));
        }
        if let Some(contract) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
            return Some(abi_signature_from_syntax(
                db,
                key,
                &contract.parameters,
                contract.return_type.as_ref(),
            ));
        }
        node.of::<beskid_analysis::syntax::TestDefinition>()
            .map(|_| {
                Ok(ItemSignature {
                    parameters: Arc::from([]),
                    result: SemanticTypeId::UNIT,
                })
            })
    })?
    .transpose()
}

/// Derive one direct call's ABI signature from its declaration and exact source arguments.
///
/// Generic declaration parameters are substituted only when every use is constrained by a
/// current argument with a generation-safe ABI type. This intentionally does not introduce
/// general inference or monomorphization: unsupported generic shapes remain unavailable.
#[salsa::tracked]
fn call_abi_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        Some(call_abi_signature_for_call(db, key))
    })?
    .transpose()
}

fn call_abi_signature_for_call(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<ItemSignature, SemanticError> {
    let declaration = match call_lowering(db, key)? {
        Some(CallLowering::Direct(declaration)) => declaration,
        Some(CallLowering::CorelibService(service)) => {
            return corelib_service_abi_signature(service)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Dynamic) => {
            return dispatch_builtin_abi_signature(db, key)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Runtime(_)) | None => {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
    };
    let declaration_syntax = db
        .syntax_unit(declaration.unit)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let Some(function) = declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>()
    else {
        return item_abi_signature(db, declaration)?
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
    };
    if function.generics.is_empty() {
        return item_abi_signature(db, declaration)?
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
    }

    let arguments =
        call_arguments(db, key)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    if arguments.len() != function.parameters.len() {
        return Err(SemanticError::unavailable("call_abi_signature"));
    }
    let generic_names = function
        .generics
        .iter()
        .map(|generic| generic.node.name.as_str())
        .collect::<Vec<_>>();
    let mut substitutions = HashMap::new();
    if let Some(instantiation) = generic_call_instantiation(db, key)?
        && !instantiation.arguments.is_empty()
    {
        if instantiation.arguments.len() != generic_names.len() {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
        for (generic, argument) in generic_names.iter().zip(instantiation.arguments.iter()) {
            substitutions.insert((*generic).to_owned(), *argument);
        }
    }
    // A bare integer starts at the language default `i32`, but carries no explicit ABI suffix.
    // Keep that distinction while inferring a generic call: a later exact argument can select
    // the binding and the bare literal can inherit it if its magnitude fits.
    let mut provisional_integer_substitutions = HashSet::new();
    for (parameter, argument) in function.parameters.iter().zip(arguments.iter().copied()) {
        let actual = abi_type(db, argument)?
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        if let Some(generic) = generic_type_name(&parameter.node.ty.node, &generic_names) {
            let bare_integer = unsuffixed_integer_literal(db, argument)?;
            match substitutions.get(generic).copied() {
                None => {
                    substitutions.insert(generic.to_owned(), actual);
                    if bare_integer {
                        provisional_integer_substitutions.insert(generic.to_owned());
                    }
                }
                Some(existing) if existing == actual => {}
                Some(existing)
                    if bare_integer && integer_literal_fits_abi(db, argument, existing)? => {}
                Some(_) if provisional_integer_substitutions.remove(generic) => {
                    substitutions.insert(generic.to_owned(), actual);
                }
                Some(_) => return Err(SemanticError::unavailable("call_abi_signature")),
            }
        } else if abi_type_from_syntax(db, declaration, &parameter.node.ty.node)? != actual {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
    }
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions))
        .collect::<Result<Vec<_>, _>>()?;
    let result = function
        .return_type
        .as_ref()
        .map_or(Ok(SemanticTypeId::UNIT), |return_type| {
            generic_abi_type(db, declaration, &return_type.node, &substitutions)
        })?;
    Ok(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

/// Whether a source expression is a bare integer literal without an ABI suffix.
///
/// This follows only singleton syntax wrappers, so compound arithmetic, casts, calls, and arrays
/// never inherit an ABI representation from a surrounding call.
fn unsuffixed_integer_literal(db: &dyn Db, key: AstNodeKey) -> Result<bool, SemanticError> {
    Ok(integer_literal_text(db, key)?.is_some())
}

/// Prove that one bare integer literal fits the ABI representation selected elsewhere in its
/// generic call. Explicitly suffixed literals never reach this helper.
fn integer_literal_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    let Some(text) = integer_literal_text(db, key)? else {
        return Ok(false);
    };
    let value = text.replace('_', "").parse::<u64>().ok();
    Ok(match expected {
        SemanticTypeId::I32 => value.is_some_and(|value| i32::try_from(value).is_ok()),
        SemanticTypeId::I64 => value.is_some_and(|value| i64::try_from(value).is_ok()),
        SemanticTypeId::U8 => value.is_some_and(|value| u8::try_from(value).is_ok()),
        SemanticTypeId::WORD => value.is_some(),
        _ => false,
    })
}

fn integer_literal_text(db: &dyn Db, key: AstNodeKey) -> Result<Option<Arc<str>>, SemanticError> {
    let Some(literal) = literal_fact(db, key)? else {
        let Some(children) = child_nodes(db, key)? else {
            return Ok(None);
        };
        let [child] = children.as_ref() else {
            return Ok(None);
        };
        return integer_literal_text(db, *child);
    };
    match literal {
        LiteralFact::Integer(text) if !integer_has_explicit_abi_suffix(&text) => Ok(Some(text)),
        _ => Ok(None),
    }
}

fn integer_has_explicit_abi_suffix(text: &str) -> bool {
    matches!(
        text.rsplit_once('_').map(|(_, suffix)| suffix),
        Some("i32" | "i64" | "u8")
    )
}

#[salsa::tracked]
fn call_argument_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some((|| {
            let mut current = key.node;
            while let Some(parent) = index
                .metadata_for(key.generation, current)
                .and_then(|meta| meta.parent)
            {
                let parent_key = AstNodeKey {
                    node: parent,
                    ..key
                };
                if index.kind(parent)
                    == Some(beskid_analysis::syntax_query::NodeKind::CallExpression)
                {
                    let arguments = call_arguments(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    let argument_index = arguments.iter().position(|argument| {
                        let mut descendant = key.node;
                        loop {
                            if descendant == argument.node {
                                return true;
                            }
                            let Some(next) = index
                                .metadata_for(key.generation, descendant)
                                .and_then(|meta| meta.parent)
                            else {
                                return false;
                            };
                            descendant = next;
                        }
                    });
                    let Some(argument_index) = argument_index else {
                        current = parent;
                        continue;
                    };
                    if integer_literal_text(db, arguments[argument_index])?.is_none() {
                        return Err(SemanticError::unavailable("call_argument_abi_type"));
                    }
                    let signature = call_abi_signature(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    return signature
                        .parameters
                        .get(argument_index)
                        .copied()
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"));
                }
                current = parent;
            }
            Err(SemanticError::unavailable("call_argument_abi_type"))
        })())
    })?
    .transpose()
}

/// ABI signature for a soft runtime builtin reached as [`CallLowering::Dynamic`].
///
/// Only names with a dispatch route receive a signature; this never grants Corelib-service or
/// canonical-runtime intrinsic authority.
fn dispatch_builtin_abi_signature(db: &dyn Db, key: AstNodeKey) -> Option<ItemSignature> {
    let symbol = dispatch_builtin_symbol(db, key).ok().flatten()?;
    let (_, spec) = beskid_analysis::builtins::builtin_specs()
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.runtime_symbol == symbol.0)?;
    let parameters = spec
        .params
        .iter()
        .copied()
        .map(builtin_type_to_semantic)
        .collect::<Option<Vec<_>>>()?;
    let result = builtin_type_to_semantic(spec.returns)?;
    Some(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

fn builtin_type_to_semantic(ty: beskid_analysis::builtins::BuiltinType) -> Option<SemanticTypeId> {
    use beskid_analysis::builtins::BuiltinType;
    Some(match ty {
        BuiltinType::String => SemanticTypeId::STRING,
        BuiltinType::Ptr => SemanticTypeId::POINTER,
        BuiltinType::Usize => SemanticTypeId::WORD,
        BuiltinType::U64 => SemanticTypeId::I64,
        BuiltinType::Unit => SemanticTypeId::UNIT,
        BuiltinType::Never => SemanticTypeId::NEVER,
    })
}
/// ABI facts for the compiler-embedded Corelib syscall facade. These are deliberately available
/// only after [`CallLowering::CorelibService`] has proved the current source corpus; user source
/// that merely spells one of these names remains dynamic and receives no import signature.
fn corelib_service_abi_signature(service: CorelibService) -> Option<ItemSignature> {
    let (parameters, result) = match service.name {
        "__syscall_write" => (
            vec![SemanticTypeId::I64, SemanticTypeId::STRING],
            SemanticTypeId::I64,
        ),
        "__syscall_read" => (
            vec![SemanticTypeId::I64, SemanticTypeId::I64],
            SemanticTypeId::STRING,
        ),
        "__syscall_write_bytes" => (
            vec![SemanticTypeId::I64, SemanticTypeId::POINTER],
            SemanticTypeId::I64,
        ),
        "__syscall_read_bytes" => (
            vec![SemanticTypeId::I64, SemanticTypeId::I64],
            SemanticTypeId::POINTER,
        ),
        "__panic_str" => (vec![SemanticTypeId::STRING], SemanticTypeId::NEVER),
        _ => return None,
    };
    Some(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

fn generic_abi_type(
    db: &dyn Db,
    declaration: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<String, SemanticTypeId>,
) -> Result<SemanticTypeId, SemanticError> {
    let generic = match syntax_type {
        beskid_analysis::syntax::Type::Complex(path) => {
            let [segment] = path.node.segments.as_slice() else {
                return abi_type_from_syntax(db, declaration, syntax_type);
            };
            segment
                .node
                .type_args
                .is_empty()
                .then_some(segment.node.name.node.name.as_str())
        }
        _ => None,
    };
    generic
        .and_then(|name| substitutions.get(name).copied())
        .map(Ok)
        .unwrap_or_else(|| abi_type_from_syntax(db, declaration, syntax_type))
}

fn generic_type_name<'a>(
    syntax_type: &'a beskid_analysis::syntax::Type,
    generics: &[&str],
) -> Option<&'a str> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    segment
        .node
        .type_args
        .is_empty()
        .then_some(name)
        .filter(|name| generics.contains(name))
}

fn abi_signature_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| abi_type_from_syntax(db, key, &parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        abi_type_from_syntax(db, key, &return_type.node)
    })?;
    Ok(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

#[salsa::tracked]
fn abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
            return Some(abi_type_for_expression(db, program, index, key, expression));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
            return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
        }
        if let Some(statement) = node.of::<beskid_analysis::syntax::LetStatement>() {
            return Some(
                statement
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| SemanticError::unavailable("abi_type"))
                    .and_then(|ty| abi_type_from_syntax(db, key, &ty.node)),
            );
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
            return Some(abi_type_for_local_path(
                db,
                program,
                index,
                key,
                &path.path.node,
            ));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
            return abi_local_declaration_type(db, program, index, key, key.node);
        }
        if node
            .of::<beskid_analysis::syntax::CallExpression>()
            .is_some()
        {
            let lowering = match call_lowering(db, key) {
                Ok(Some(lowering)) => lowering,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            let CallLowering::Direct(_) = lowering else {
                return Some(Err(SemanticError::unavailable("abi_type")));
            };
            let signature = match call_abi_signature(db, key) {
                Ok(Some(signature)) => signature,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            return Some(Ok(signature.result));
        }
        if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
            return Some(abi_type_from_syntax(db, key, syntax_type));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        None
    })?
    .transpose()
}

fn abi_type_for_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Expression;

    match expression {
        Expression::Literal(literal) => Ok(semantic_type_for_literal(&literal.node.literal.node)),
        Expression::Path(path) => {
            abi_type_for_local_path(db, program, index, key, &path.node.path.node)
        }
        Expression::Grouped(grouped) => {
            abi_type_for_expression(db, program, index, key, &grouped.node.expr.node)
        }
        Expression::Call(call) => {
            let call = index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(call),
                )
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            call_abi_signature(db, call)?
                .map(|signature| signature.result)
                .ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Binary(binary) => {
            let binary_key = index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(binary),
                )
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            abi_type(db, binary_key)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

fn abi_type_for_binary_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    binary: &beskid_analysis::syntax::BinaryExpression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = index
        .direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(binary.left.as_ref()),
        )
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right = index
        .direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(binary.right.as_ref()),
        )
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let left_type = abi_type(db, left)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right_type = abi_type(db, right)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    use beskid_analysis::syntax::BinaryOp;
    match binary.op.node {
        BinaryOp::Add
            if left_type == SemanticTypeId::STRING && right_type == SemanticTypeId::STRING =>
        {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Eq | BinaryOp::NotEq
            if left_type == SemanticTypeId::STRING && right_type == SemanticTypeId::STRING =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Or | BinaryOp::And
            if left_type == SemanticTypeId::BOOL && right_type == SemanticTypeId::BOOL =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte
            if left_type == right_type =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left_type == right_type && primitive_numeric(left_type) =>
        {
            Ok(left_type)
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

fn abi_type_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Type;

    match syntax_type {
        Type::Primitive(_) => semantic_type_from_syntax(syntax_type),
        Type::Complex(path) => nominal_aggregate_abi_type(db, key, &path.node),
        Type::Array(_) | Type::Function { .. } => Err(SemanticError::unavailable("abi_type")),
    }
}

#[salsa::tracked]
fn aggregate_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        let definition = node.of::<beskid_analysis::syntax::TypeDefinition>()?;
        Some(
            definition
                .fields
                .iter()
                .map(|field| aggregate_field_layout(db, program, index, key, field))
                .collect::<Result<Vec<_>, SemanticError>>()
                .map(|fields| AggregateLayoutFact {
                    fields: fields.into(),
                }),
        )
    })?
    .transpose()
}

#[salsa::tracked]
fn aggregate_literal_declaration_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        node.of::<beskid_analysis::syntax::StructLiteralExpression>()
            .and_then(|literal| {
                resolve_nominal_layout_declaration(db, program, index, key, &literal.path.node)
            })
    })
}

#[salsa::tracked]
fn aggregate_field_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateFieldAccess> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [receiver, field] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !receiver.node.type_args.is_empty() || !field.node.type_args.is_empty() {
            return None;
        }
        let (declaration, receiver) = nominal_local_receiver_declaration(
            db,
            program,
            index,
            key,
            receiver.node.name.node.name.as_str(),
        )?;
        let layout = aggregate_layout(db, declaration).ok().flatten()?;
        let index = layout
            .fields
            .iter()
            .position(|(name, _)| name.as_ref() == field.node.name.node.name)
            .and_then(|index| u32::try_from(index).ok())?;
        Some(Ok(AggregateFieldAccess {
            declaration,
            receiver,
            index,
        }))
    })?
    .transpose()
}

/// Resolve the smallest generation-safe member receiver: an unqualified local with an explicit
/// nominal parameter or let annotation. Calls, inferred locals, and chained receivers remain
/// unavailable rather than reconstructing retired HIR type information.
fn nominal_local_receiver_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local)?;
    let annotation = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.ty.node),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    resolve_type_declaration(db, key, &path.node).map(|declaration| (declaration, receiver))
}

#[salsa::tracked]
fn enum_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        let definition = node.of::<beskid_analysis::syntax::EnumDefinition>()?;
        Some(
            definition
                .variants
                .iter()
                .map(|variant| {
                    variant
                        .node
                        .fields
                        .iter()
                        .map(|field| aggregate_field_layout(db, program, index, key, field))
                        .collect::<Result<Vec<_>, SemanticError>>()
                        .map(|fields| EnumVariantLayoutFact {
                            name: Arc::from(variant.node.name.node.name.as_str()),
                            fields: fields.into(),
                        })
                })
                .collect::<Result<Vec<_>, SemanticError>>()
                .map(|variants| EnumLayoutFact {
                    variants: variants.into(),
                }),
        )
    })?
    .transpose()
}

#[salsa::tracked]
fn enum_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let declaration = resolve_type_declaration(db, key, &constructor.path.node.type_path.node)
            .ok_or_else(|| SemanticError::unavailable("enum_constructor"));
        let declaration = match declaration {
            Ok(declaration) => declaration,
            Err(error) => return Some(Err(error)),
        };
        let layout = match enum_layout(db, declaration) {
            Ok(Some(layout)) => layout,
            Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant_index) = layout
            .variants
            .iter()
            .position(|variant| variant.name.as_ref() == variant_name)
        else {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        };
        let variant = &layout.variants[variant_index];
        if variant.fields.len() != constructor.args.len() || variant.fields.len() > 1 {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        }
        let payload = constructor
            .args
            .first()
            .map(|argument| {
                index
                    .direct_child_id(
                        program,
                        key.node,
                        beskid_analysis::syntax_query::DynNodeRef::from(argument),
                    )
                    .map(|node| AstNodeKey {
                        node: normalized_expression_node(index, node),
                        ..key
                    })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
            })
            .transpose();
        let variant_index = match u32::try_from(variant_index) {
            Ok(variant_index) => variant_index,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(payload.map(|payload| EnumConstructorFact {
            declaration,
            variant_index,
            payload,
        }))
    })?
    .transpose()
}

#[salsa::tracked]
fn enum_match_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumMatchFact> {
    with_node(db, syntax, key, |program, index, node| {
        let expression = node.of::<beskid_analysis::syntax::MatchExpression>()?;
        let mut declaration = None;
        let mut arms = Vec::with_capacity(expression.arms.len());
        for arm in &expression.arms {
            if arm.node.guard.is_some() {
                return Some(Err(SemanticError::unavailable("enum_match")));
            }
            let arm_node = index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(arm),
                )
                .ok_or_else(|| SemanticError::unavailable("enum_match"));
            let arm_node = match arm_node {
                Ok(arm_node) => arm_node,
                Err(error) => return Some(Err(error)),
            };
            let body = index
                .direct_child_id(
                    program,
                    arm_node,
                    beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.value),
                )
                .map(|body| AstNodeKey {
                    node: normalized_expression_node(index, body),
                    ..key
                })
                .ok_or_else(|| SemanticError::unavailable("enum_match"));
            let body = match body {
                Ok(body) => body,
                Err(error) => return Some(Err(error)),
            };
            let variant_index = match &arm.node.pattern.node {
                beskid_analysis::syntax::Pattern::Wildcard => Ok(None),
                beskid_analysis::syntax::Pattern::Enum(pattern)
                    if pattern.node.items.is_empty() =>
                {
                    let candidate =
                        resolve_type_declaration(db, key, &pattern.node.path.node.type_path.node)
                            .ok_or_else(|| SemanticError::unavailable("enum_match"));
                    let candidate = match candidate {
                        Ok(candidate) => candidate,
                        Err(error) => return Some(Err(error)),
                    };
                    if declaration.is_some_and(|current| current != candidate) {
                        return Some(Err(SemanticError::unavailable("enum_match")));
                    }
                    declaration = Some(candidate);
                    let layout = match enum_layout(db, candidate) {
                        Ok(Some(layout)) => layout,
                        Ok(None) | Err(_) => {
                            return Some(Err(SemanticError::unavailable("enum_match")));
                        }
                    };
                    let name = pattern.node.path.node.variant.node.name.as_str();
                    layout
                        .variants
                        .iter()
                        .position(|variant| variant.name.as_ref() == name)
                        .and_then(|index| u32::try_from(index).ok())
                        .ok_or_else(|| SemanticError::unavailable("enum_match"))
                        .map(Some)
                }
                _ => Err(SemanticError::unavailable("enum_match")),
            };
            let variant_index = match variant_index {
                Ok(variant_index) => variant_index,
                Err(error) => return Some(Err(error)),
            };
            arms.push(EnumMatchArmFact {
                variant_index,
                body,
            });
        }
        declaration.map(|declaration| {
            Ok(EnumMatchFact {
                declaration,
                arms: arms.into(),
            })
        })
    })?
    .transpose()
}

fn aggregate_field_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
) -> Result<(Arc<str>, AggregateFieldShape), SemanticError> {
    if field.node.kind != beskid_analysis::syntax::FieldKind::Value {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    let shape = match &field.node.ty.node {
        beskid_analysis::syntax::Type::Primitive(_) => {
            AggregateFieldShape::Scalar(semantic_type_from_syntax(&field.node.ty.node)?)
        }
        beskid_analysis::syntax::Type::Complex(path) => AggregateFieldShape::Nominal(
            resolve_nominal_layout_declaration(db, program, index, key, &path.node)
                .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?,
        ),
        // Arrays are heap-backed reference values in ABI v5, including empty literal payloads.
        beskid_analysis::syntax::Type::Array(_) => {
            AggregateFieldShape::Scalar(SemanticTypeId::POINTER)
        }
        _ => return Err(SemanticError::unavailable("aggregate_layout")),
    };
    Ok((Arc::from(field.node.name.node.name.as_str()), shape))
}

fn resolve_nominal_layout_declaration(
    _db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    let candidates = index
        .metadata()
        .iter()
        .filter_map(|metadata| {
            let node = index.node_at(program, metadata.id)?;
            let matches = node
                .of::<beskid_analysis::syntax::TypeDefinition>()
                .is_some_and(|definition| definition.name.node.name == name)
                || node
                    .of::<beskid_analysis::syntax::EnumDefinition>()
                    .is_some_and(|definition| definition.name.node.name == name);
            matches.then_some(AstNodeKey {
                node: metadata.id,
                ..key
            })
        })
        .collect::<Vec<_>>();
    let [declaration] = candidates.as_slice() else {
        return None;
    };
    Some(*declaration)
}

fn nominal_aggregate_abi_type(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    resolve_type_declaration(db, key, path)
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    Ok(SemanticTypeId::POINTER)
}

fn abi_type_for_local_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    let [segment] = path.segments.as_slice() else {
        return Err(SemanticError::unavailable("abi_type"));
    };
    if !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("abi_type"));
    }
    let declaration = resolve_lexical_declaration(
        program,
        index,
        key.node,
        segment.node.name.node.name.as_str(),
    )
    .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    abi_local_declaration_type(db, program, index, key, declaration)
        .unwrap_or_else(|| Err(SemanticError::unavailable("abi_type")))
}

fn abi_local_declaration_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| abi_type_from_syntax(db, key, &parameter.ty.node)),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .map(|statement| {
                statement.type_annotation.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("abi_type")),
                    |syntax_type| abi_type_from_syntax(db, key, &syntax_type.node),
                )
            }),
        _ => None,
    }
}

fn resolve_type_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, module_path) = path.segments.split_last()?;
    let generic_arity = name.node.type_args.len();
    let name = name.node.name.node.name.as_str();
    if module_path.is_empty() {
        let mut candidates = Vec::new();
        if let Some(local) = unique_type_in_unit(db, key.unit, key.generation, name, generic_arity)
        {
            candidates.push(local);
        }
        let import_targets = {
            let registry = db
                .syntax_dependency_registry()
                .lock()
                .expect("syntax dependency registry");
            registry
                .imports
                .get(&(key.unit, key.generation))
                .into_iter()
                .flatten()
                .map(|import| import.target)
                .collect::<Vec<_>>()
        };
        candidates.extend(import_targets.into_iter().filter_map(|target| {
            unique_exported_type_in_unit(db, target, key.generation, name, generic_arity)
        }));
        let [declaration] = candidates.as_slice() else {
            return None;
        };
        return Some(*declaration);
    }
    let module_path = module_path
        .iter()
        .map(|segment| segment.node.name.node.name.as_str())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    unique_exported_type_in_unit(
        db,
        resolve_qualified_module_unit(db, key, &module_path)?,
        key.generation,
        name,
        generic_arity,
    )
}

/// Resolve a public type member through its defining syntax unit or explicit public re-exports.
fn unique_exported_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(candidate) =
            unique_public_type_in_unit(db, current, generation, name, generic_arity)
        {
            candidates.push(candidate);
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    let [candidate] = candidates.as_slice() else {
        return None;
    };
    Some(*candidate)
}

fn unique_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let matches = index
        .metadata()
        .iter()
        .map(|metadata| metadata.id)
        .filter(|candidate| {
            index.node_at(program, *candidate).is_some_and(|node| {
                node.of::<beskid_analysis::syntax::TypeDefinition>()
                    .is_some_and(|definition| {
                        definition.name.node.name == name
                            && definition.generics.len() == generic_arity
                    })
                    || node
                        .of::<beskid_analysis::syntax::EnumDefinition>()
                        .is_some_and(|definition| {
                            definition.name.node.name == name
                                && definition.generics.len() == generic_arity
                        })
            })
        })
        .collect::<Vec<_>>();
    let [node] = matches.as_slice() else {
        return None;
    };
    Some(AstNodeKey {
        unit,
        generation,
        node: *node,
    })
}

fn unique_public_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let matches = index
        .metadata()
        .iter()
        .map(|metadata| metadata.id)
        .filter(|candidate| {
            index.node_at(program, *candidate).is_some_and(|node| {
                node.of::<beskid_analysis::syntax::TypeDefinition>()
                    .is_some_and(|definition| {
                        definition.visibility.node == beskid_analysis::syntax::Visibility::Public
                            && definition.name.node.name == name
                            && definition.generics.len() == generic_arity
                    })
                    || node
                        .of::<beskid_analysis::syntax::EnumDefinition>()
                        .is_some_and(|definition| {
                            definition.visibility.node
                                == beskid_analysis::syntax::Visibility::Public
                                && definition.name.node.name == name
                                && definition.generics.len() == generic_arity
                        })
            })
        })
        .collect::<Vec<_>>();
    let [node] = matches.as_slice() else {
        return None;
    };
    Some(AstNodeKey {
        unit,
        generation,
        node: *node,
    })
}

fn semantic_type_from_syntax(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::{PrimitiveType, Type};

    match syntax_type {
        Type::Primitive(primitive) => Ok(match primitive.node {
            PrimitiveType::Bool => SemanticTypeId::BOOL,
            PrimitiveType::I32 => SemanticTypeId::I32,
            PrimitiveType::I64 => SemanticTypeId::I64,
            PrimitiveType::U8 => SemanticTypeId::U8,
            PrimitiveType::Pointer => SemanticTypeId::POINTER,
            PrimitiveType::Word => SemanticTypeId::WORD,
            PrimitiveType::F64 => SemanticTypeId::F64,
            PrimitiveType::Char => SemanticTypeId::CHAR,
            PrimitiveType::String => SemanticTypeId::STRING,
            PrimitiveType::Unit => SemanticTypeId::UNIT,
            PrimitiveType::Never => SemanticTypeId::NEVER,
        }),
        Type::Complex(_) | Type::Array(_) | Type::Function { .. } => {
            Err(SemanticError::unavailable("item_signature"))
        }
    }
}

#[salsa::tracked]
fn closure_environment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureEnvironment> {
    with_node(db, syntax, key, |program, index, node| {
        closure_environment_for_node(program, index, key, node)
    })?
    .transpose()
}

fn closure_environment_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ClosureEnvironment, SemanticError>> {
    let lambda = node.of::<beskid_analysis::syntax::LambdaExpression>()?;
    let parameters = match lambda
        .parameters
        .iter()
        .map(|parameter| {
            index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(parameter),
                )
                .ok_or_else(|| SemanticError::unavailable("closure_environment"))
                .and_then(|parameter| {
                    index
                        .children(parameter)
                        .and_then(|children| {
                            children.iter().copied().find(|child| {
                                index.kind(*child)
                                    == Some(beskid_analysis::syntax_query::NodeKind::Identifier)
                            })
                        })
                        .map(|node| AstNodeKey { node, ..key })
                        .ok_or_else(|| SemanticError::unavailable("closure_environment"))
                })
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(parameters) => parameters,
        Err(error) => return Some(Err(error)),
    };
    let captures = match closure_captures(program, index, key) {
        Ok(captures) => captures.into(),
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(ClosureEnvironment {
        parameters: parameters.into(),
        captures,
    }))
}

#[salsa::tracked]
fn closure_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureSignature> {
    with_node(db, syntax, key, |program, index, node| {
        closure_signature_for_node(db, program, index, key, node)
    })?
    .transpose()
}

fn closure_signature_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ClosureSignature, SemanticError>> {
    let lambda = node.of::<beskid_analysis::syntax::LambdaExpression>()?;
    let body = index
        .direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(lambda.body.as_ref()),
        )
        .map(|node| AstNodeKey {
            node: normalized_expression_node(index, node),
            ..key
        })?;
    let callable = match callable_signature_for_node(db, program, index, key, node) {
        Some(Ok(callable)) => callable,
        Some(Err(error)) => return Some(Err(error)),
        None => return None,
    };
    let environment = match closure_environment_for_node(program, index, key, node) {
        Some(Ok(environment)) => environment,
        Some(Err(error)) => return Some(Err(error)),
        None => return None,
    };
    let fields = environment
        .captures
        .iter()
        .map(|capture| {
            local_declaration_type(program, index, capture.declaration.node)
                .unwrap_or_else(|| Err(SemanticError::unavailable("closure_signature")))
                .map(|abi_type| ClosureEnvironmentField {
                    capture: *capture,
                    abi_type,
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|mut fields| {
            fields.sort_by_key(|field| {
                (
                    field.capture.slot.owner.node.0,
                    field.capture.slot.index,
                    field.capture.declaration.node.0,
                )
            });
            fields
        });
    Some(fields.map(|fields| ClosureSignature {
        lambda: key,
        body,
        callable,
        environment: ClosureEnvironmentAbiShape {
            fields: fields.into(),
            pointer_map: ClosurePointerMapRequirement::RuntimeDescriptorRequired,
        },
        lowering: ClosureLoweringStatus::NotLowered,
        allocation: ClosureAllocationStatus::NotAllocated,
    }))
}

#[salsa::tracked]
fn closure_call_target_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureCallTarget> {
    let lambda = with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        index
            .direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            )
            .map(|node| AstNodeKey {
                node: normalized_expression_node(index, node),
                ..key
            })
    })?;
    let Some(lambda) = lambda else {
        return Ok(None);
    };
    let Some(signature) = closure_signature_tracked(db, syntax, lambda)? else {
        return Ok(None);
    };
    Ok(Some(ClosureCallTarget {
        call: key,
        lambda: signature.lambda,
        body: signature.body,
        callable: signature.callable,
    }))
}

fn closure_captures(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    lambda: AstNodeKey,
) -> Result<Vec<ClosureCapture>, SemanticError> {
    let mut captures: Vec<ClosureCapture> = Vec::new();
    for path_id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::PathExpression) {
        if !is_ancestor(index, lambda.node, path_id) {
            continue;
        }
        let Some(node) = index.node_at(program, path_id) else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(declaration) = resolve_lexical_declaration(
            program,
            index,
            path_id,
            path.path
                .node
                .segments
                .first()
                .map(|segment| segment.node.name.node.name.as_str())
                .unwrap_or_default(),
        ) else {
            continue;
        };
        if path.path.node.segments.len() != 1 || is_ancestor(index, lambda.node, declaration) {
            continue;
        }
        let declaration = AstNodeKey {
            node: declaration,
            ..lambda
        };
        if captures
            .iter()
            .any(|capture| capture.declaration == declaration)
        {
            continue;
        }
        let Some(slot) = local_slot_for_declaration(index, declaration) else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(span) = node.span() else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let class = capture_storage_class(program, index, declaration)?;
        captures.push(ClosureCapture {
            declaration,
            slot: slot?,
            class,
            span,
        });
    }
    Ok(captures)
}

#[salsa::tracked]
fn capture_storage_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CaptureStorage> {
    with_node(db, syntax, key, |program, index, node| {
        capture_storage_for_node(program, index, key, node)
    })?
    .transpose()
}

fn capture_storage_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CaptureStorage, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(
        program,
        index,
        key.node,
        segment.node.name.node.name.as_str(),
    )?;
    let declaration = AstNodeKey {
        node: declaration,
        ..key
    };
    let span = node.span()?;
    Some(
        capture_storage_class(program, index, declaration).map(|class| CaptureStorage {
            declaration,
            class,
            span,
        }),
    )
}

fn capture_storage_class(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
) -> Result<CaptureStorageClass, SemanticError> {
    let parent = parent_node(index, declaration.node)
        .ok_or_else(|| SemanticError::unavailable("capture_storage"))?;
    let mutable = index
        .node_at(program, parent)
        .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
        .is_some_and(|binding| binding.mutable);
    let semantic_type = local_declaration_type(program, index, declaration.node)
        .unwrap_or_else(|| Err(SemanticError::unavailable("capture_storage")))?;
    Ok(if mutable || semantic_type == SemanticTypeId::POINTER {
        CaptureStorageClass::StackReference
    } else {
        CaptureStorageClass::TransferableValue
    })
}

#[salsa::tracked]
fn callable_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |program, index, node| {
        callable_signature_for_node(db, program, index, key, node)
    })?
    .transpose()
}

fn callable_signature_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ItemSignature, SemanticError>> {
    if let Some(signature) = item_signature_for_node(node) {
        return Some(signature);
    }
    if let Some(lambda) = node.of::<beskid_analysis::syntax::LambdaExpression>() {
        let parameters = lambda
            .parameters
            .iter()
            .map(|parameter| {
                parameter.node.ty.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("callable_signature")),
                    |ty| semantic_type_from_syntax(&ty.node),
                )
            })
            .collect::<Result<Vec<_>, _>>();
        let result = semantic_type_for_expression(program, index, key.node, &lambda.body.node);
        return Some(parameters.and_then(|parameters| {
            result.map(|result| ItemSignature {
                parameters: parameters.into(),
                result,
            })
        }));
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        return callable_signature_for_path(db, program, index, key, &path.path.node);
    }
    if let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>()
        && let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node
    {
        return callable_signature_for_path(db, program, index, key, &path.node.path.node);
    }
    None
}

fn callable_signature_for_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<Result<ItemSignature, SemanticError>> {
    let declaration = resolve_item_declaration(db, program, index, key, path)?;
    let declaration = index.node_at(program, declaration.node)?;
    item_signature_for_node(declaration)
}

#[salsa::tracked]
fn spawn_target_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnTarget> {
    with_node(db, syntax, key, |program, index, node| {
        let spawn = node.of::<beskid_analysis::syntax::SpawnExpression>()?;
        let callee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(spawn.callee.as_ref()),
        )?;
        let callee = AstNodeKey {
            node: normalized_expression_node(index, callee),
            ..key
        };
        let callee = match spawn_entry_operand(program, index, callee) {
            Ok(callee) => callee,
            Err(error) => return Some(Err(error)),
        };
        let captures = if index.kind(callee.node)
            == Some(beskid_analysis::syntax_query::NodeKind::LambdaExpression)
        {
            match closure_captures(program, index, callee) {
                Ok(captures) => captures.into(),
                Err(error) => return Some(Err(error)),
            }
        } else {
            Arc::from([])
        };
        Some(Ok(SpawnTarget { callee, captures }))
    })?
    .transpose()
}

/// Resolve the fiber entry operand for one spawn callee expression.
///
/// Empty-arg `spawn Entry()` sugar unwraps to `Entry`, matching production lowering. Call
/// expressions that still carry arguments remain the CallExpression node so legality can reject
/// them without inventing a trampoline.
fn spawn_entry_operand(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    callee: AstNodeKey,
) -> Result<AstNodeKey, SemanticError> {
    let Some(node) = index.node_at(program, callee.node) else {
        return Ok(callee);
    };
    let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>() else {
        return Ok(callee);
    };
    if !call.args.is_empty() {
        return Ok(callee);
    }
    let entry = index
        .direct_child_id(
            program,
            callee.node,
            beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
        )
        .ok_or_else(|| SemanticError::unavailable("spawn_target"))?;
    Ok(AstNodeKey {
        node: normalized_expression_node(index, entry),
        ..callee
    })
}

fn normalized_expression_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    mut node: beskid_analysis::syntax::AstNodeId,
) -> beskid_analysis::syntax::AstNodeId {
    while matches!(
        index.kind(node),
        Some(
            beskid_analysis::syntax_query::NodeKind::Expression
                | beskid_analysis::syntax_query::NodeKind::GroupedExpression
        )
    ) {
        let Some(child) = index
            .children(node)
            .and_then(|children| children.first())
            .copied()
        else {
            break;
        };
        node = child;
    }
    node
}

#[salsa::tracked]
fn spawn_legality_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnLegality> {
    let target = spawn_target_tracked(db, syntax, key)?;
    let Some(target) = target else {
        return Ok(None);
    };
    let span = node_span_tracked(db, syntax, key)?
        .ok_or_else(|| SemanticError::unavailable("spawn_legality"))?;
    let index = syntax.syntax_index(db);
    if index.kind(target.callee.node)
        == Some(beskid_analysis::syntax_query::NodeKind::CallExpression)
    {
        // Non-empty `spawn Entry(args)` left the CallExpression in place; fail closed before
        // signature lookup so parameterized callees are not misdiagnosed as TargetRequiresArguments.
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::CalleeArgumentsUnsupported,
                span,
                capture: None,
            }]),
        }));
    }
    let signature = callable_signature_tracked(db, syntax, target.callee)?;
    let Some(signature) = signature else {
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetNotCallable,
                span,
                capture: None,
            }]),
        }));
    };

    if !signature.parameters.is_empty() {
        return Ok(Some(SpawnLegality {
            target,
            result: Some(signature.result),
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetRequiresArguments,
                span,
                capture: None,
            }]),
        }));
    }

    let capture = spawn_stack_capture(db, syntax, target.callee, &target.captures)?;
    let diagnostics = capture.map_or_else(
        || Arc::from([]),
        |capture| {
            Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::StackReferenceEscapesSpawn,
                span: capture.span,
                capture: Some(capture),
            }])
        },
    );
    Ok(Some(SpawnLegality {
        target,
        result: Some(signature.result),
        span,
        diagnostics,
    }))
}

#[salsa::tracked]
fn spawn_entry_validation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnEntryValidation> {
    let Some(legality) = spawn_legality_tracked(db, syntax, key)? else {
        return Ok(None);
    };
    let callable = callable_signature_tracked(db, syntax, legality.target.callee)?;
    let is_zero_argument_entry = callable
        .as_ref()
        .is_some_and(|callable| callable.parameters.is_empty())
        && legality.is_legal();
    Ok(Some(SpawnEntryValidation {
        spawn: key,
        target: legality.target.callee,
        callable,
        is_zero_argument_entry,
        diagnostics: legality.diagnostics,
    }))
}

fn spawn_stack_capture(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    lambda: AstNodeKey,
    captures: &[ClosureCapture],
) -> Result<Option<CaptureStorage>, SemanticError> {
    let index = syntax.syntax_index(db);
    if index.kind(lambda.node) != Some(beskid_analysis::syntax_query::NodeKind::LambdaExpression) {
        return Ok(None);
    }
    for path in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::PathExpression) {
        if !is_ancestor(index, lambda.node, path) {
            continue;
        }
        let reference = AstNodeKey {
            node: path,
            ..lambda
        };
        let Some(storage) = capture_storage_tracked(db, syntax, reference)? else {
            continue;
        };
        if storage.class == CaptureStorageClass::StackReference
            && captures
                .iter()
                .any(|capture| capture.declaration == storage.declaration)
        {
            return Ok(Some(storage));
        }
    }
    Ok(None)
}

#[salsa::tracked]
fn runtime_intrinsic_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsic> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return Some(Err(SemanticError::unavailable("runtime_intrinsic")));
        };
        if path.node.path.node.segments.len() == 1
            && resolve_lexical_declaration(
                program,
                index,
                key.node,
                path.node.path.node.segments[0].node.name.node.name.as_str(),
            )
            .is_some()
        {
            return Some(Err(SemanticError::unavailable("runtime_intrinsic")));
        }
        let segments = path
            .node
            .path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect::<Vec<_>>();
        beskid_analysis::builtins::builtin_for_path(&segments)
            .map(|(index, _)| {
                u32::try_from(index)
                    .map(RuntimeIntrinsic)
                    .map_err(|_| SemanticError::unavailable("runtime_intrinsic"))
            })
            .or_else(|| Some(Err(SemanticError::unavailable("runtime_intrinsic"))))
    })?
    .transpose()
}

#[salsa::tracked]
fn runtime_intrinsic_name_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsicName> {
    with_node(db, syntax, key, |_program, _index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        if path.node.path.node.segments.len() != 1 {
            return None;
        }
        Some(Ok(RuntimeIntrinsicName(Arc::from(
            path.node.path.node.segments[0].node.name.node.name.as_str(),
        ))))
    })?
    .transpose()
}

#[salsa::tracked]
fn node_kind_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<IndexedNodeKind> {
    with_node(db, syntax, key, |_program, _index, node| {
        Some(node.node_kind())
    })
}

#[salsa::tracked]
fn child_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some(
            index
                .children(key.node)?
                .iter()
                .map(|node| AstNodeKey { node: *node, ..key })
                .collect::<Vec<_>>()
                .into(),
        )
    })
}

#[salsa::tracked]
fn literal_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<LiteralFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        let literal = node.of::<beskid_analysis::syntax::Literal>()?;
        Some(match literal {
            beskid_analysis::syntax::Literal::Integer(value) => {
                LiteralFact::Integer(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Float(value) => {
                LiteralFact::Float(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::String(value) => {
                LiteralFact::String(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Char(value) => {
                LiteralFact::Char(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Bool(value) => LiteralFact::Bool(*value),
        })
    })
}

#[salsa::tracked]
fn node_span_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SourceSpan> {
    with_node(db, syntax, key, |_program, _index, node| node.span())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchBuiltinSymbol(pub &'static str);

#[salsa::tracked]
fn dispatch_builtin_symbol_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<DispatchBuiltinSymbol> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let lowering =
            call_lowering_for_node(db, program, index, key, node).and_then(|result| result.ok())?;
        if lowering != CallLowering::Dynamic {
            return None;
        }
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        if path.node.path.node.segments.len() != 1 {
            return None;
        }
        let name = path.node.path.node.segments[0].node.name.node.name.as_str();
        let (_, spec) = beskid_analysis::builtins::builtin_for_path(&[name.to_owned()])?;
        dispatch_route_for_symbol(spec.runtime_symbol)?;
        Some(Ok(DispatchBuiltinSymbol(spec.runtime_symbol)))
    })?
    .transpose()
}

pub fn dispatch_builtin_symbol(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<DispatchBuiltinSymbol> {
    with_registered_syntax(db, key, dispatch_builtin_symbol_tracked)
}

#[salsa::tracked]
fn operator_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<OperatorFact> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return operator_fact_for_binary(db, program, index, key, binary);
        }
        if let Some(unary) = node.of::<beskid_analysis::syntax::UnaryExpression>() {
            return Some(unary_operator(unary.op.node));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryOp>() {
            return Some(binary_operator(*binary));
        }
        node.of::<beskid_analysis::syntax::UnaryOp>()
            .copied()
            .map(unary_operator)
    })
}

fn binary_operator(operator: beskid_analysis::syntax::BinaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::BinaryOp::Or => OperatorFact::Or,
        beskid_analysis::syntax::BinaryOp::And => OperatorFact::And,
        beskid_analysis::syntax::BinaryOp::IdentityEq => OperatorFact::IdentityEq,
        beskid_analysis::syntax::BinaryOp::IdentityNotEq => OperatorFact::IdentityNotEq,
        beskid_analysis::syntax::BinaryOp::Eq => OperatorFact::Eq,
        beskid_analysis::syntax::BinaryOp::NotEq => OperatorFact::NotEq,
        beskid_analysis::syntax::BinaryOp::Lt => OperatorFact::Lt,
        beskid_analysis::syntax::BinaryOp::Lte => OperatorFact::Lte,
        beskid_analysis::syntax::BinaryOp::Gt => OperatorFact::Gt,
        beskid_analysis::syntax::BinaryOp::Gte => OperatorFact::Gte,
        beskid_analysis::syntax::BinaryOp::Add => OperatorFact::Add,
        beskid_analysis::syntax::BinaryOp::Sub => OperatorFact::Sub,
        beskid_analysis::syntax::BinaryOp::Mul => OperatorFact::Mul,
        beskid_analysis::syntax::BinaryOp::Div => OperatorFact::Div,
        beskid_analysis::syntax::BinaryOp::Mod => OperatorFact::Mod,
    }
}

fn operator_fact_for_binary(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    binary: &beskid_analysis::syntax::BinaryExpression,
) -> Option<OperatorFact> {
    let left = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(binary.left.as_ref()),
    )?;
    let right = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(binary.right.as_ref()),
    )?;
    let left_key = AstNodeKey { node: left, ..key };
    let right_key = AstNodeKey { node: right, ..key };
    let left_type = abi_type(db, left_key).ok().flatten();
    let right_type = abi_type(db, right_key).ok().flatten();
    let result_type = abi_type(db, key).ok().flatten();
    let involves_string = [left_type, right_type, result_type]
        .into_iter()
        .any(|ty| ty == Some(SemanticTypeId::STRING));
    if involves_string {
        return Some(match binary.op.node {
            beskid_analysis::syntax::BinaryOp::Add => OperatorFact::StringAdd,
            beskid_analysis::syntax::BinaryOp::Eq => OperatorFact::StringEq,
            beskid_analysis::syntax::BinaryOp::NotEq => OperatorFact::StringNotEq,
            op => binary_operator(op),
        });
    }
    Some(binary_operator(binary.op.node))
}

fn unary_operator(operator: beskid_analysis::syntax::UnaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::UnaryOp::Neg => OperatorFact::Neg,
        beskid_analysis::syntax::UnaryOp::Not => OperatorFact::Not,
    }
}

#[salsa::tracked]
fn item_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            return index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(&function.body),
                )
                .map(|node| AstNodeKey { node, ..key });
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            return index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(&method.body),
                )
                .map(|node| AstNodeKey { node, ..key });
        }
        if node
            .of::<beskid_analysis::syntax::TestDefinition>()
            .is_some()
        {
            return Some(key);
        }
        None
    })
}

/// Return the executable statements of a test item in source order.
///
/// A test definition also owns visibility, name, and optional metadata children.  ISLE function
/// emission must enumerate only its statement body, never those declaration children.
#[salsa::tracked]
fn test_statement_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let test = node.of::<beskid_analysis::syntax::TestDefinition>()?;
        Some(
            test.statements
                .iter()
                .map(|statement| {
                    let wrapper = index
                        .direct_child_id(
                            program,
                            key.node,
                            beskid_analysis::syntax_query::DynNodeRef::from(statement),
                        )
                        .ok_or_else(|| SemanticError::unavailable("test_statement_nodes"))?;
                    let children = index
                        .children(wrapper)
                        .ok_or_else(|| SemanticError::unavailable("test_statement_nodes"))?;
                    let [statement] = children else {
                        return Err(SemanticError::unavailable("test_statement_nodes"));
                    };
                    Ok(AstNodeKey {
                        node: *statement,
                        ..key
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Arc::from),
        )
    })?
    .transpose()
}

/// Return executable block statements without syntax-index wrapper nodes.
#[salsa::tracked]
fn block_statement_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let block = node.of::<beskid_analysis::syntax::Block>()?;
        Some(
            block
                .statements
                .iter()
                .map(|statement| {
                    let wrapper = index
                        .direct_child_id(
                            program,
                            key.node,
                            beskid_analysis::syntax_query::DynNodeRef::from(statement),
                        )
                        .ok_or_else(|| SemanticError::unavailable("block_statement_nodes"))?;
                    let [statement] = index
                        .children(wrapper)
                        .ok_or_else(|| SemanticError::unavailable("block_statement_nodes"))?
                    else {
                        return Err(SemanticError::unavailable("block_statement_nodes"));
                    };
                    Ok(AstNodeKey {
                        node: *statement,
                        ..key
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Arc::from),
        )
    })?
    .transpose()
}

#[salsa::tracked]
fn item_name_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<str>> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::FunctionDefinition>()
            .map(|definition| Arc::from(definition.name.node.name.as_str()))
            .or_else(|| {
                node.of::<beskid_analysis::syntax::MethodDefinition>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::TestDefinition>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::ContractMethodSignature>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
    })
}

#[salsa::tracked]
fn item_export_symbol_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ExportSymbol> {
    with_node(db, syntax, key, |_program, _index, node| {
        let definition = node.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        let export = definition
            .attributes
            .iter()
            .find(|attribute| attribute.node.name.node.name == "Export")?;
        if definition.visibility.node != beskid_analysis::syntax::Visibility::Public {
            return Some(Err(SemanticError::new(
                "`[Export]` applies to `pub` functions only",
            )));
        }
        let raw = export.node.arguments.iter().find_map(|argument| {
            if argument.node.name.node.name != "Symbol" {
                return None;
            }
            let beskid_analysis::syntax::Expression::Literal(literal) = &argument.node.value.node
            else {
                return None;
            };
            let beskid_analysis::syntax::Literal::String(value) = &literal.node.literal.node else {
                return None;
            };
            value.strip_prefix('"')?.strip_suffix('"')
        })?;
        Some(Ok(ExportSymbol(Arc::from(raw))))
    })
    .and_then(|export| export.transpose())
}

#[salsa::tracked]
fn test_item_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<TestItem> {
    with_node(db, syntax, key, |_program, _index, node| {
        let definition = node.of::<beskid_analysis::syntax::TestDefinition>()?;
        let mut module_path = Vec::new();
        let mut parent = parent_node(_index, key.node);
        while let Some(current) = parent {
            if let Some(module) = _index
                .node_at(_program, current)
                .and_then(|node| node.of::<beskid_analysis::syntax::InlineModule>())
            {
                module_path.push(module.name.node.name.clone());
            }
            parent = parent_node(_index, current);
        }
        module_path.reverse();
        let qualified_name = if module_path.is_empty() {
            definition.name.node.name.clone()
        } else {
            format!("{}::{}", module_path.join("::"), definition.name.node.name)
        };
        let mut tags = Vec::new();
        let mut group = None;
        if let Some(meta) = &definition.meta {
            for entry in &meta.node.entries {
                match entry.node.name.node.name.as_str() {
                    "group" => group = test_string_literal(&entry.node.value),
                    "tags" => {
                        tags = test_string_literal(&entry.node.value)
                            .into_iter()
                            .flat_map(|value| {
                                value
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|tag| !tag.is_empty())
                                    .map(Arc::<str>::from)
                                    .collect::<Vec<_>>()
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        let mut skip_condition = None;
        let mut skip_reason = None;
        if let Some(skip) = &definition.skip {
            for entry in &skip.node.entries {
                match entry.node.name.node.name.as_str() {
                    "condition" => skip_condition = test_bool_literal(&entry.node.value),
                    "reason" => skip_reason = test_string_literal(&entry.node.value),
                    _ => {}
                }
            }
        }
        Some(TestItem {
            name: Arc::from(definition.name.node.name.as_str()),
            qualified_name: Arc::from(qualified_name),
            tags: Arc::from(tags),
            group,
            skip_condition,
            skip_reason,
            selection_span: definition.name.span,
        })
    })
}

fn test_string_literal(
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<Arc<str>> {
    let beskid_analysis::syntax::Expression::Literal(literal) = &expression.node else {
        return None;
    };
    beskid_analysis::syntax::try_decode_string_literal(&literal.node.literal.node).map(Arc::from)
}

fn test_bool_literal(
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<bool> {
    let beskid_analysis::syntax::Expression::Literal(literal) = &expression.node else {
        return None;
    };
    let beskid_analysis::syntax::Literal::Bool(value) = &literal.node.literal.node else {
        return None;
    };
    Some(*value)
}

#[salsa::tracked]
fn direct_callees_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        if node
            .of::<beskid_analysis::syntax::FunctionDefinition>()
            .is_none()
            && node
                .of::<beskid_analysis::syntax::TestDefinition>()
                .is_none()
            && node
                .of::<beskid_analysis::syntax::MethodDefinition>()
                .is_none()
        {
            return None;
        }
        Some(direct_callees_for_item(db, program, index, key))
    })?
    .transpose()
}

fn direct_callees_for_item(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    item: AstNodeKey,
) -> Result<Arc<[AstNodeKey]>, SemanticError> {
    let mut callees = Vec::new();
    for call_id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        if !is_ancestor(index, item.node, call_id) {
            continue;
        }
        let Some(call_node) = index.node_at(program, call_id) else {
            continue;
        };
        // Reachability enumerates Direct callees only. Unavailable/dynamic call
        // classifications (e.g. extern contract members) are not Direct edges and
        // must not fail closed the whole entrypoint walk.
        let Some(Ok(lowering)) = call_lowering_for_node(
            db,
            program,
            index,
            AstNodeKey {
                node: call_id,
                ..item
            },
            call_node,
        ) else {
            continue;
        };
        if let CallLowering::Direct(declaration) = lowering
            && !callees.contains(&declaration)
        {
            // Extern contract methods are import leaves (no syntax body). Including them
            // as Direct reachability edges makes reachable_items fail closed.
            if extern_contract_import_for_declaration(db, declaration).is_some() {
                continue;
            }
            callees.push(declaration);
        }
    }
    Ok(callees.into())
}

#[salsa::tracked]
fn reachable_items_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    program: AstNodeKey,
    entry: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    if !syntax.accepts_key(db, program)
        || syntax
            .syntax_index(db)
            .metadata_for(program.generation, program.node)
            .is_none()
    {
        return Ok(None);
    }
    let Some(entry_syntax) = db.syntax_unit(entry.unit) else {
        return Ok(None);
    };
    if !entry_syntax.accepts_key(db, entry)
        || entry_syntax
            .syntax_index(db)
            .metadata_for(entry.generation, entry.node)
            .is_none()
    {
        return Ok(None);
    }
    if syntax.syntax_index(db).kind(program.node)
        != Some(beskid_analysis::syntax_query::NodeKind::Program)
        || !matches!(
            entry_syntax.syntax_index(db).kind(entry.node),
            Some(
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::TestDefinition
            )
        )
    {
        return Ok(None);
    }

    fn visit(
        db: &dyn Db,
        item: AstNodeKey,
        reachable: &mut Vec<AstNodeKey>,
    ) -> Result<(), SemanticError> {
        if reachable.contains(&item) {
            return Ok(());
        }
        reachable.push(item);
        let item_syntax = db
            .syntax_unit(item.unit)
            .ok_or_else(|| SemanticError::unavailable("reachable_items"))?;
        if !item_syntax.accepts_key(db, item) {
            return Err(SemanticError::unavailable("reachable_items"));
        }
        let callees = direct_callees_tracked(db, item_syntax, item)?
            .ok_or_else(|| SemanticError::unavailable("reachable_items"))?;
        for callee in callees.iter().copied() {
            visit(db, callee, reachable)?;
        }
        Ok(())
    }

    let mut reachable = Vec::new();
    visit(db, entry, &mut reachable)?;
    Ok(Some(reachable.into()))
}

fn with_node<T>(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    query: impl FnOnce(
        &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
        &beskid_analysis::syntax_query::SyntaxIndex,
        beskid_analysis::syntax_query::DynNodeRef<'_>,
    ) -> Option<T>,
) -> SemanticQueryResult<T> {
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let expanded = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    if index.generation() != key.generation
        || index.metadata_for(key.generation, key.node).is_none()
    {
        return Ok(None);
    }
    let Some(node) = index.node_at(expanded, key.node) else {
        return Ok(None);
    };
    Ok(query(expanded, index, node))
}

/// Resolve a single-segment value path to an exact function declaration in lexical module scope.
///
/// Local declarations shadow item names. Ambiguous, qualified, generic, and unresolved paths
/// contain no item fact. Stale, unregistered, and non-path nodes also contain no fact.
pub fn resolved_item(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedItem> {
    with_registered_syntax(db, key, resolved_item_tracked)
}

/// Enumerate exact syntax-backed callable completion candidates for one current generation.
///
/// Generation-safe completion facts for lexical locals, types, imported members, and
/// explicitly annotated nominal receivers. Inferred receivers remain unavailable.
pub fn completion_candidates(
    db: &dyn Db,
    key: AstNodeKey,
    context: CompletionContext,
) -> SemanticQueryResult<Arc<[CompletionCandidate]>> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let Some(file) = db
        .file_registry()
        .lock()
        .expect("file registry")
        .get(key.unit.path(db))
        .copied()
    else {
        return Ok(None);
    };
    let source = file.text(db);
    if context.cursor > source.len()
        || context.replacement_start > context.replacement_end
        || context.replacement_end > source.len()
        || !source.is_char_boundary(context.cursor)
        || !source.is_char_boundary(context.replacement_start)
        || !source.is_char_boundary(context.replacement_end)
    {
        return Ok(None);
    }
    let prefix = &source[context.replacement_start..context.replacement_end];
    let before = &source[..context.replacement_start];
    let mut candidates = Vec::new();
    if let Some(before_dot) = before.strip_suffix('.') {
        let alias = before_dot
            .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .next()
            .unwrap_or_default();
        let import_target = {
            let registry = db
                .syntax_dependency_registry()
                .lock()
                .expect("syntax dependency registry");
            registry
                .imports
                .get(&(key.unit, key.generation))
                .and_then(|imports| imports.iter().find(|import| import.binding == alias))
                .map(|import| import.target)
        };
        if let Some(target) = import_target {
            let Some(target_syntax) = db.syntax_unit(target) else {
                return Ok(None);
            };
            if target_syntax.generation(db) != key.generation {
                return Ok(None);
            }
            let program = target_syntax.expanded_program(db);
            let index = target_syntax.syntax_index(db);
            push_unit_member_candidates(&mut candidates, program, index, prefix, &context);
        } else {
            let program = syntax.expanded_program(db);
            let index = syntax.syntax_index(db);
            let reference = deepest_node_containing_offset(index, context.cursor)
                .unwrap_or(key.node);
            let lookup = AstNodeKey {
                node: reference,
                ..key
            };
            let Some((declaration, _)) =
                nominal_local_receiver_declaration(db, program, index, lookup, alias)
            else {
                return Ok(None);
            };
            push_nominal_receiver_candidates(
                db,
                &mut candidates,
                declaration,
                prefix,
                &context,
            );
        }
    } else {
        let program = syntax.expanded_program(db);
        let index = syntax.syntax_index(db);
        let reference =
            deepest_node_containing_offset(index, context.cursor).unwrap_or(key.node);
        push_lexical_local_candidates(
            &mut candidates,
            program,
            index,
            reference,
            prefix,
            &context,
        );
        push_unit_type_candidates(&mut candidates, program, index, prefix, &context);
        for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition) {
            if let Some(function) = index
                .node_at(program, id)
                .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
            {
                push_completion_candidate(
                    &mut candidates,
                    function.name.node.name.as_str(),
                    CompletionKind::Function,
                    None,
                    prefix,
                    &context,
                );
            }
        }
    }
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
    Ok(Some(candidates.into()))
}

fn push_completion_candidate(
    candidates: &mut Vec<CompletionCandidate>,
    label: &str,
    kind: CompletionKind,
    detail: Option<&str>,
    prefix: &str,
    context: &CompletionContext,
) {
    if !label.starts_with(prefix) {
        return;
    }
    candidates.push(CompletionCandidate {
        label: Arc::from(label),
        kind,
        detail: detail.map(Arc::from),
        replacement_start: context.replacement_start,
        replacement_end: context.replacement_end,
    });
}

fn deepest_node_containing_offset(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    offset: usize,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut best: Option<(usize, beskid_analysis::syntax::AstNodeId)> = None;
    for metadata in index.metadata() {
        let Some(span) = metadata.span else {
            continue;
        };
        if offset < span.start || offset > span.end {
            continue;
        }
        let length = span.end.saturating_sub(span.start);
        if best.is_none_or(|(best_length, best_id)| {
            length < best_length || (length == best_length && metadata.id.0 > best_id.0)
        }) {
            best = Some((length, metadata.id));
        }
    }
    best.map(|(_, id)| id)
}

fn push_lexical_local_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    prefix: &str,
    context: &CompletionContext,
) {
    for declaration in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier) {
        if local_declaration_scope(index, declaration, reference).is_none() {
            continue;
        }
        let Some(identifier) = index
            .node_at(program, declaration)
            .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        else {
            continue;
        };
        push_completion_candidate(
            candidates,
            identifier.name.as_str(),
            CompletionKind::Variable,
            Some("local"),
            prefix,
            context,
        );
    }
}

fn push_unit_type_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    prefix: &str,
    context: &CompletionContext,
) {
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::TypeDefinition) {
        if let Some(definition) = index
            .node_at(program, id)
            .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        {
            push_completion_candidate(
                candidates,
                definition.name.node.name.as_str(),
                CompletionKind::Type,
                Some("type"),
                prefix,
                context,
            );
        }
    }
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::EnumDefinition) {
        if let Some(definition) = index
            .node_at(program, id)
            .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        {
            push_completion_candidate(
                candidates,
                definition.name.node.name.as_str(),
                CompletionKind::Type,
                Some("enum"),
                prefix,
                context,
            );
        }
    }
}

fn push_unit_member_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    prefix: &str,
    context: &CompletionContext,
) {
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition) {
        if let Some(function) = index
            .node_at(program, id)
            .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        {
            push_completion_candidate(
                candidates,
                function.name.node.name.as_str(),
                CompletionKind::Function,
                None,
                prefix,
                context,
            );
        }
    }
    push_unit_type_candidates(candidates, program, index, prefix, context);
}

fn push_nominal_receiver_candidates(
    db: &dyn Db,
    candidates: &mut Vec<CompletionCandidate>,
    declaration: AstNodeKey,
    prefix: &str,
    context: &CompletionContext,
) {
    let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else {
        return;
    };
    if declaration_syntax.generation(db) != declaration.generation {
        return;
    }
    let program = declaration_syntax.expanded_program(db);
    let index = declaration_syntax.syntax_index(db);
    let Some(definition) = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return;
    };
    for field in &definition.fields {
        push_completion_candidate(
            candidates,
            field.node.name.node.name.as_str(),
            CompletionKind::Field,
            Some("field"),
            prefix,
            context,
        );
    }
    for method in &definition.methods {
        push_completion_candidate(
            candidates,
            method.node.name.node.name.as_str(),
            CompletionKind::Method,
            Some("method"),
            prefix,
            context,
        );
    }
    if let Some(children) = index.children(declaration.node) {
        for child in children.iter().copied() {
            if let Some(method) = index
                .node_at(program, child)
                .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
            {
                push_completion_candidate(
                    candidates,
                    method.name.node.name.as_str(),
                    CompletionKind::Method,
                    Some("method"),
                    prefix,
                    context,
                );
            }
        }
    }
}

/// Resolve a single-segment value path to its generation-safe lexical declaration key.
///
/// Function and method parameters, lets, lambda parameters, for iterators, and match bindings are
/// supported. Out-of-scope, self-initializing, qualified, generic, and unresolved paths contain no
/// local fact.
pub fn resolved_local(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedLocal> {
    with_registered_syntax(db, key, resolved_local_tracked)
}

/// Return the deterministic owner-qualified slot for an exact local declaration identifier.
///
/// Function and method parameters precede body declarations in expanded-AST order. Lambda frames
/// have distinct owner keys. Stale, unregistered, ownerless, and non-declaration identifiers
/// contain no fact.
pub fn local_slot(db: &dyn Db, declaration: AstNodeKey) -> SemanticQueryResult<LocalSlot> {
    with_registered_syntax(db, declaration, local_slot_tracked)
}

/// Return the exact mutable local destination for a simple assignment expression.
///
/// The fact rejects immutable declarations, non-path targets, qualified paths, compound targets,
/// and stale or unregistered syntax. Codegen uses it as the only authority for local writes.
pub fn mutable_local_assignment(
    db: &dyn Db,
    assignment: AstNodeKey,
) -> SemanticQueryResult<MutableLocalAssignment> {
    with_registered_syntax(db, assignment, mutable_local_assignment_tracked)
}

/// Return primitive types proven by literals, explicit syntax, or exact lexical declarations.
///
/// Complex declarations and expression shapes requiring inference remain explicitly unavailable.
/// Stale, unregistered, and non-typable nodes contain no fact.
pub fn node_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, node_type_tracked)
}

/// Return the exact root expression keys of positional call arguments in source order.
///
/// Empty calls contain an empty fact. Stale, unregistered, and non-call nodes contain no fact.
/// A current argument that cannot be mapped through the authoritative syntax index is explicitly
/// unavailable.
pub fn call_arguments(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, call_arguments_tracked)
}

/// Return exact current-generation bounds for the syntax-only `range(start, end)` loop form.
pub fn range_for_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RangeForFact> {
    with_registered_syntax(db, key, range_for_fact_tracked)
}

/// Return the iterator declaration identity and element type for one current `ForStatement`.
///
/// Only the syntax-only `range(start, end)` iterable proves an element type. Stale generations,
/// unregistered nodes, and non-for statements contain no fact.
pub fn for_iterator_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ForIteratorFact> {
    with_registered_syntax(db, key, for_iterator_fact_tracked)
}

/// Return the declaration identifier for the receiver of an exact `local.Method()` path.
///
/// The fact exists only when the local has an explicit nominal parameter or let annotation and
/// that nominal type declares exactly one matching method. Static paths, inferred locals,
/// extension methods, overloads, and chained receivers remain unavailable.
pub fn nominal_member_receiver(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, nominal_member_receiver_tracked)
}

/// Classify call shapes whose lowering is certain from expanded syntax alone.
///
/// Immediate lambda calls are dynamic. Exactly resolved single-segment function calls are direct.
/// Ambiguous, shadowed, unresolved, member, runtime, and other call shapes remain explicitly
/// unavailable. Stale, unregistered, and non-call nodes contain no fact.
pub fn call_lowering(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CallLowering> {
    with_registered_syntax(db, key, call_lowering_tracked)
}

/// Return the exact declared generic target for one current call with explicit terminal type
/// arguments. Arity mismatches, stale generations, and inferred calls remain unavailable.
pub fn generic_call_instantiation(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallInstantiation> {
    with_registered_syntax(db, key, generic_call_instantiation_tracked)
}

/// Return the exact source-derived ABI specialization for one generic direct call.
///
/// Inferred generic arguments are accepted only when every ABI type is proven by the current
/// call arguments.  The returned declaration plus signature is suitable for a mangled module
/// identity and never consults legacy HIR lowering.
pub fn generic_call_specialization(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallSpecialization> {
    with_registered_syntax(db, key, generic_call_specialization_tracked)
}

/// Return numeric cast intents proven by an exact typed-let constraint.
///
/// Inferred, complex, non-numeric, and other unported coercion contexts remain explicitly
/// unavailable. Stale, unregistered, and non-expression nodes contain no fact.
pub fn cast_intents(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[CastIntent]>> {
    with_registered_syntax(db, key, cast_intents_tracked)
}

/// Return AST-derived fall-through facts for executable nodes in the current generation.
///
/// Loops are conservative because their body may execute zero times. Stale, unregistered, and
/// non-executable nodes contain no fact.
pub fn control_flow(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ControlFlow> {
    with_registered_syntax(db, key, control_flow_tracked)
}

/// Return exact callable signatures whose types have generation-independent primitive identities.
///
/// Complex, array, and function types remain unavailable until their Salsa type identities are
/// ported. Stale, unregistered, and non-callable nodes contain no fact.
pub fn item_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, item_signature_tracked)
}

/// Return the scalar ABI representation signature proven by current source syntax.
pub fn item_abi_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, item_abi_signature_tracked)
}

/// Return the exact ABI signature selected by one direct call expression.
///
/// Generic parameters are substituted only from matching current argument facts; no HIR type
/// result or inferred fallback participates in this boundary.
pub fn call_abi_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, call_abi_signature_tracked)
}

/// Return target-neutral source field shapes for a nominal `type` definition.
pub fn aggregate_layout(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AggregateLayoutFact> {
    with_registered_syntax(db, key, aggregate_layout_tracked)
}

/// Return the current nominal `type` declaration constructed by a struct literal.
pub fn aggregate_literal_declaration(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, aggregate_literal_declaration_tracked)
}

/// Return the exact field selected by a direct nominal local receiver member expression.
pub fn aggregate_field_access(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateFieldAccess> {
    with_registered_syntax(db, key, aggregate_field_access_tracked)
}

/// Return target-neutral source variants and field shapes for a nominal `enum` definition.
pub fn enum_layout(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumLayoutFact> {
    with_registered_syntax(db, key, enum_layout_tracked)
}

/// Return the exact source enum constructor selection for the current syntax generation.
///
/// Constructors with multiple payload fields remain unavailable until the generated ISLE enum
/// emitter has an equally explicit multi-field payload representation.
pub fn enum_constructor(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumConstructorFact> {
    with_registered_syntax(db, key, enum_constructor_tracked)
}

/// Return the exact source enum declaration and arms selected by one `match` expression.
///
/// Guarded and payload-destructuring arms remain unavailable until generated ISLE owns their
/// binding and control-flow representation.
pub fn enum_match(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumMatchFact> {
    with_registered_syntax(db, key, enum_match_tracked)
}

/// Return the scalar ABI representation for one current syntax node.
pub fn abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, abi_type_tracked)
}

/// Return the exact call-parameter ABI selected for one bare integer argument.
///
/// Only a singleton unsuffixed integer argument of a direct call receives this contextual fact;
/// all other expressions remain unavailable rather than being implicitly coerced.
pub fn call_argument_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, call_argument_abi_type_tracked)
}

/// Return the exact lambda parameters and outer lexical captures in source order.
///
/// Captures never include declarations owned by the lambda itself. Stale, unregistered, and
/// non-lambda nodes contain no fact.
pub fn closure_environment(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureEnvironment> {
    with_registered_syntax(db, key, closure_environment_tracked)
}

/// Return a generation-bound lambda signature, body key, and deterministic capture ABI shape.
///
/// The shape requires a runtime pointer-map descriptor, but this fact deliberately reports that
/// no generated lowering or closure allocation exists yet. Generic and inferred callable shapes
/// remain unavailable rather than consulting HIR. Stale, unregistered, and non-lambda nodes
/// contain no fact.
pub fn closure_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ClosureSignature> {
    with_registered_syntax(db, key, closure_signature_tracked)
}

/// Return the direct lambda call target selected by one call expression.
///
/// Calls through local bindings and all dynamic closure dispatch remain unavailable; this query
/// does not infer a runtime closure object. Stale, unregistered, and non-call nodes contain no
/// fact.
pub fn closure_call_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ClosureCallTarget> {
    with_registered_syntax(db, key, closure_call_target_tracked)
}

/// Return the exact spawn operand and any captures required when it is a lambda expression.
///
/// Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SpawnTarget> {
    with_registered_syntax(db, key, spawn_target_tracked)
}

/// Return source-owned storage provenance for one exact local-path use.
///
/// Mutable bindings and native pointers are stack references and must not cross a spawn
/// boundary. This is a source-provenance fact only; it does not claim rooted closure storage.
/// Stale, unregistered, non-local, and non-path nodes contain no fact.
pub fn capture_storage(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CaptureStorage> {
    with_registered_syntax(db, key, capture_storage_tracked)
}

/// Return the callable signature proven by the current syntax generation.
///
/// Functions, methods, tests, typed lambdas, direct item paths, and direct item calls are
/// supported. Inferred/complex callable shapes remain unavailable rather than consulting HIR.
pub fn callable_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, callable_signature_tracked)
}

/// Return authoritative spawn legality, result, and precise source diagnostics.
///
/// The fact never inspects HIR. A non-callable target gets `TargetNotCallable`; an entry with
/// parameters gets `TargetRequiresArguments`; a mutable or native-pointer closure capture gets
/// `StackReferenceEscapesSpawn`. Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_legality(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SpawnLegality> {
    with_registered_syntax(db, key, spawn_legality_tracked)
}

/// Return source-only zero-argument spawn-entry validation for the current syntax generation.
///
/// This validation does not claim a generated trampoline, closure allocation, or runtime fiber
/// object. Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_entry_validation(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnEntryValidation> {
    with_registered_syntax(db, key, spawn_entry_validation_tracked)
}

/// Return the manifest-owned intrinsic index for an exact, unshadowed builtin call.
///
/// Unknown, dynamic, and lexically shadowed calls remain explicitly unavailable. Stale or
/// unregistered keys contain no fact.
pub fn runtime_intrinsic(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RuntimeIntrinsic> {
    with_registered_syntax(db, key, runtime_intrinsic_tracked)
}

/// Return an unprivileged direct-call spelling for the codegen runtime import gate.
///
/// The name alone does not authorize an ABI import; only a canonical-runtime typed program can
/// turn it into one.
pub fn runtime_intrinsic_name(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsicName> {
    with_registered_syntax(db, key, runtime_intrinsic_name_tracked)
}

pub fn node_kind(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<IndexedNodeKind> {
    with_registered_syntax(db, key, node_kind_tracked)
}

pub fn child_nodes(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, child_nodes_tracked)
}

pub fn literal_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<LiteralFact> {
    with_registered_syntax(db, key, literal_fact_tracked)
}

pub fn node_span(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SourceSpan> {
    with_registered_syntax(db, key, node_span_tracked)
}

pub fn operator_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<OperatorFact> {
    with_registered_syntax(db, key, operator_fact_tracked)
}

pub fn item_body(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, item_body_tracked)
}

/// Return the exact executable statement nodes for a current syntax test definition.
pub fn test_statement_nodes(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, test_statement_nodes_tracked)
}

/// Return executable statements for a current block in source order.
pub fn block_statement_nodes(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, block_statement_nodes_tracked)
}

/// Return the exact declared name for a current syntax function, method, or test item.
pub fn item_name(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<str>> {
    with_registered_syntax(db, key, item_name_tracked)
}

/// Return the explicitly declared linker symbol for a current syntax function.
pub fn item_export_symbol(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ExportSymbol> {
    with_registered_syntax(db, key, item_export_symbol_tracked)
}

/// Return CLI-facing metadata for one current syntax `test` item.
pub fn test_item(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<TestItem> {
    with_registered_syntax(db, key, test_item_tracked)
}

/// Return unique direct function callees in expanded-syntax order.
///
/// Dynamic calls do not add an edge. Any unresolved call makes the result explicitly unavailable
/// so an incomplete graph cannot masquerade as complete.
pub fn direct_callees(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, direct_callees_tracked)
}

/// Traverse direct function calls from an entry using generation-safe declaration keys.
///
/// The result is deterministic depth-first preorder and includes the entry. Recursive cycles are
/// visited once. Missing or unresolved call facts propagate explicit unavailability.
pub fn reachable_items(
    db: &dyn Db,
    program: AstNodeKey,
    entry: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    let Some(syntax) = db.syntax_unit(program.unit) else {
        return Ok(None);
    };
    reachable_items_tracked(db, syntax, program, entry)
}
