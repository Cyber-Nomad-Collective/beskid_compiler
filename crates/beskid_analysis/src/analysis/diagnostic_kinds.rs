mod code;
mod help;
mod internal;
mod label;
mod message;
mod render;
mod severity;
mod type_names;

pub use internal::{INTERNAL_ERROR_REPORT, humanize_construct};
pub use render::issue_diagnostic;
pub use type_names::{display_type, display_type_pair};

use crate::syntax::SpanInfo;

#[derive(Debug, Clone)]
pub enum SemanticIssueKind {
    // ── Definition / duplicate-name diagnostics ──
    DuplicateDefinitionName {
        name: String,
        previous: SpanInfo,
    },
    DuplicateEnumVariant {
        name: String,
        previous: SpanInfo,
    },
    DuplicateContractMethod {
        name: String,
        previous: SpanInfo,
    },
    DuplicateItemName {
        name: String,
        previous: SpanInfo,
    },
    UnknownTypeInDefinition {
        type_name: String,
    },
    ConflictingEmbeddedContractMethod {
        contract_name: String,
        method_name: String,
    },

    // ── Import / visibility / naming diagnostics ──
    AmbiguousImport {
        name: String,
        previous: SpanInfo,
    },
    UnknownImportPath {
        path: String,
        /// The module the path's prefix names when the last segment is an item of it
        /// (`use Module.Item`): `use` imports modules only.
        module: Option<String>,
    },
    UseBeforeDeclaration {
        name: String,
    },
    InvalidSyntaxSpan {
        context: String,
    },
    UnresolvedSyntaxValuePath,
    UnresolvedSyntaxTypePath,
    NonCanonicalSyntaxControlFlow {
        message: String,
    },
    DuplicateAttributeDeclarationTarget {
        target: String,
        previous: SpanInfo,
    },
    UnknownAttributeDeclarationTarget {
        target: String,
        allowed: Vec<String>,
    },
    /// E1510. `declared` is false when no attribute declaration of that name is in scope and
    /// the name is not compiler-recognized.
    AttributeTargetNotAllowed {
        attribute: String,
        target: String,
        allowed: Vec<String>,
        declared: bool,
    },

    VisibilityModuleNotFound {
        module_path: String,
        file_candidate: String,
        mod_candidate: String,
    },
    FileScopedModuleNotFirstItem {
        module_path: String,
    },
    DuplicateFileScopedModule {
        module_path: String,
    },
    ModuleDeclarationForbiddenInFileScopedModule,
    VisibilityViolationImportPrivate {
        name: String,
        private_span: SpanInfo,
    },
    ExtendTypePrivateMemberAccess {
        member_name: String,
        type_name: String,
        private_span: SpanInfo,
    },
    UnusedImport {
        path: String,
    },
    /// W2101: a private item nothing references. `item` spans the whole declaration (the removal
    /// fix deletes it); `kind` names the item kind (`function`, `type`, ...).
    UnusedPrivateItem {
        name: String,
        kind: String,
        item: SpanInfo,
    },

    // ── Contract conformance diagnostics ──
    ContractMethodNotFound {
        method_name: String,
        receiver_name: String,
    },
    ContractImplementationSignatureMismatch {
        method_name: String,
        expected: String,
        actual: String,
    },
    ContractMethodMissingImplementation {
        contract_name: String,
        method_name: String,
        expected: String,
    },
    ThisUsedOutsideContractOrImpl,
    UnresolvedAssociatedType {
        name: String,
    },
    ContractAssociatedTypeMissingBinding {
        contract_name: String,
        assoc_name: String,
    },
    GenericBoundNotSatisfied {
        type_name: String,
        contract_name: String,
        /// For a compiler-derived marker bound: the first offending field or the class reason.
        reason: Option<String>,
        /// For a `Serializable` bound: the byte offset of the conforming type's declaration when
        /// it is the only declaration of that name in the diagnostic's own source, where the
        /// `[Serialize]` fix inserts the attribute.
        declaration_at: Option<usize>,
    },

    /// E1214, reported once per binding at its declaration; every assignment is a related
    /// location "assigned again".
    ImmutableAssignment {
        name: String,
        binding: ImmutableBinding,
    },

    // ── Pattern / match / control-flow diagnostics ──
    MatchGuardMustBeBoolean,
    MatchArmTypeMismatch {
        expected: String,
        actual: String,
    },
    MatchNonExhaustive {
        enum_name: String,
        /// One value pattern no unguarded arm matches (`Result::Ok(_)`).
        missing_pattern: String,
    },
    DuplicatePatternBinding {
        name: String,
    },
    UnknownEnumPath {
        enum_name: String,
        variant_name: String,
    },
    PatternArityMismatch {
        expected: usize,
        actual: usize,
    },
    EnumConstructorArityMismatch {
        expected: usize,
        actual: usize,
    },
    UnqualifiedEnumConstructor {
        variant_name: String,
        enum_name: String,
    },
    BreakOutsideLoop,
    ContinueOutsideLoop,
    UnreachableCode,

    // ── Resolution diagnostics ──
    ResolveDuplicateItem {
        name: String,
        previous: SpanInfo,
    },
    ResolveDuplicateLocal {
        name: String,
        previous: SpanInfo,
    },
    ResolveUnknownValue {
        name: String,
        /// The closest name in scope ("did you mean"), when one is within the edit bound.
        suggestion: Option<String>,
    },
    ResolveUnknownType {
        name: String,
    },
    ResolveUnknownModulePath {
        path: String,
    },
    ResolveUnknownValueInModule {
        module_path: String,
        name: String,
    },
    ResolveUnknownTypeInModule {
        module_path: String,
        name: String,
    },
    ResolveInvalidConformanceTarget {
        name: String,
    },
    ResolvePrivateRuntimeBuiltin {
        name: String,
    },
    ResolvePrivateItemInModule {
        module_path: String,
        name: String,
    },
    /// A qualified path whose module path is claimed by more than one assembled module, such as
    /// the same package-native path in two packages of the dependency closure.
    ResolveAmbiguousModulePath {
        path: String,
        candidates: usize,
    },
    /// A qualified path `Module.Name` that the module serves only through its public re-exports,
    /// where `candidates` distinct items share the name at the nearest providing distance.
    ResolveAmbiguousItemInModule {
        module_path: String,
        name: String,
        candidates: usize,
    },
    ResolveShadowedLocal {
        name: String,
        previous: SpanInfo,
    },
    /// A name is unresolved but exactly one known module exports it — suggest adding a `use`.
    MissingImport {
        name: String,
        module_path: String,
    },
    /// A name is unresolved and multiple modules export it — list candidates.
    MissingImportAmbiguous {
        name: String,
        candidates: Vec<String>,
    },

    // ── Type-checking diagnostics ──
    TypeUnknownType {
        name: String,
    },
    TypeUnknownValueType,
    TypeUnknownStructType,
    TypeInvalidMemberTarget,
    TypeUnknownEnumType,
    /// E1211: a field the type does not declare. `type_name` and `fields` describe the type
    /// when the detecting fact resolved it; `suggestion` is the closest declared field.
    TypeUnknownStructField {
        name: String,
        type_name: Option<String>,
        fields: Vec<String>,
        suggestion: Option<String>,
    },
    TypeInaccessibleStructField {
        name: String,
    },
    TypeUnknownEnumVariant {
        name: String,
    },
    TypeMissingStructField {
        name: String,
    },
    TypeMissingTypeAnnotation {
        name: String,
    },
    /// E1203. `parameter` names the type parameter inference left unbound, when known.
    TypeMissingTypeArguments {
        parameter: Option<String>,
    },
    TypeGenericArgumentMismatch {
        expected: usize,
        actual: usize,
    },
    /// Two call arguments bound to the same inferred generic parameter carry different
    /// declared primitive types (e.g. `word` and `i64`) that only numeric-widening would
    /// unify. This is reported explicitly instead of letting an inconsistent specialization
    /// reach downstream ABI/codegen queries as an opaque "unavailable" failure.
    TypeGenericParameterConflict {
        parameter: String,
        first_name: String,
        second_name: String,
    },
    TypeMismatch {
        expected_name: String,
        actual_name: String,
    },
    TypeMatchArmMismatch {
        expected_name: String,
        actual_name: String,
    },
    TypeCallArityMismatch {
        expected: usize,
        actual: usize,
    },
    TypeCallArgumentMismatch {
        expected_name: String,
        actual_name: String,
    },
    TypeEnumConstructorMismatch {
        expected: usize,
        actual: usize,
    },
    TypeUnknownCallTarget,
    TypeInvalidBinaryOp,
    /// E1209: a string concatenation or interpolation operand whose type has no text form.
    TypeStringConcatOperandNotConvertible {
        found: String,
    },
    /// E1209: a compound assignment `a op= b` whose operator `a`'s type does not have.
    TypeInvalidCompoundAssignment {
        operator: String,
        type_name: String,
    },
    TypeInvalidUnaryOp,
    TypeNonBoolCondition,
    TypeUnsupportedExpression,
    TypeInvalidTryTarget,
    TypeInvalidPrimitiveConversionArgument,
    /// A scoped `use` whose acquisition, disposal, or error conversion the scoped-cleanup fact
    /// rejects; `reason` names which rule (for example `NotDisposable`, `ResourceEscapesScope`).
    ScopedCleanupRejected {
        reason: String,
    },
    /// `BSP-REQ-35580A7D7B75`: a discarded canonical growth of a `mut T[]` parameter whose body
    /// never publishes the parameter, so the caller keeps the ungrown array.
    DeadCollectionGrowth,
    /// W1240: `Array.Append` used as a statement on an array its caller does not own (a parameter
    /// that is not `mut`, a loop or pattern variable, a call result, an element). The call copies
    /// the array and appends to the copy, so discarding the result changes nothing. `owner` is the
    /// source text of the first argument and `call` the source text of the call.
    DiscardedArrayGrowth {
        owner: String,
        call: String,
        repair: DiscardedGrowthRepair,
    },
    /// A lambda that is not a spawn operand captures a mutable binding or a native pointer.
    /// Captures are by-value snapshots, so either would silently stop aliasing its source.
    LambdaCapturesStackReference {
        name: String,
    },
    // ── Transfer, capture and move band E1234–E1255 (E1251–E1255 reserved, unassigned) ──
    /// E1234: a Moved-class or `Fiber` value used after it was consumed. Related label
    /// "moved here" points at the consuming use.
    UseAfterMove {
        name: String,
    },
    /// E1235: a lambda passed to a `[Captures]` parameter assigns a captured local.
    CapturedLocalAssignedInBoundLambda {
        name: String,
    },
    /// E1236: a `[Captures]` lambda captures a value whose transfer class the bound rejects.
    CaptureNotTransferable {
        name: String,
        type_name: String,
        class: String,
    },
    /// E1237: a `[Captures]` lambda captures a Moved value without `move`.
    MovedCaptureWithoutMove {
        name: String,
    },
    /// E1238: the argument to a `[Captures]` parameter is not a lambda literal or function item.
    CapturesArgumentNotLiteral,
    /// E1239: `move` on a value of a class other than Moved (`class`), or, with `class: None`,
    /// `move` written outside an isolate crossing.
    MoveOfNonOwningName {
        name: String,
        class: Option<String>,
    },
    /// E1240: a Moved value consumed in a loop body without reassignment before the next iteration.
    MoveInLoopWithoutReassignment {
        name: String,
    },
    /// E1241: a crossing operand of Moved class that is not the only name for its value (a
    /// borrowed parameter, an aliased local or a place read). Related label "borrowed here".
    BorrowEscapes {
        name: String,
    },
    /// E1241: a value of a corelib `[ScopeBorrowed]` type returned, stored, passed to `spawn` or
    /// captured by an escaping closure. Related label "borrowed here" points at the binding.
    ScopeBorrowedEscapes {
        name: String,
        type_name: String,
    },
    /// E1242: a crossing operand of Moved class written without `move`.
    ImplicitAliasOfMovedValue {
        name: String,
    },
    /// E1243: `move x.f` on a field whose type has no canonical empty value.
    DestructiveReadWithoutEmpty {
        field: String,
        type_name: String,
    },
    /// E1244: a field of an `[Immutable]` type is not Shared.
    ImmutableFieldNotShared {
        type_name: String,
        field: String,
        class: String,
    },
    /// E1245: assignment to or escape of a field of an `[Immutable]` object outside construction.
    ImmutableFieldAssignment {
        type_name: String,
        field: String,
    },
    /// E1246: `[Copyable]`, `[Isolated]` or `[Immutable]` on a never-transferable type. `cause`
    /// names the `[NeverTransferable]` attribute or the `pointer`, function-typed or Never field.
    MarkerOnNeverTransferable {
        marker: String,
        type_name: String,
        cause: String,
    },
    /// E1247: a field class the `[Isolated]` or `[Copyable]` marker does not admit.
    MarkerFieldClassMismatch {
        marker: String,
        type_name: String,
        field: String,
        class: String,
    },
    /// E1248: a `[Captures]` bound that is not a marker contract (`bound` names it), or a
    /// `[Captures]` attribute on a parameter whose type is not a function type (`bound` is `None`).
    CapturesBoundNotMarker {
        bound: Option<String>,
    },
    /// E1249: an `inject` field read inside a task body.
    InjectInsideIsolateBody {
        field: String,
    },
    /// E1250: a method call on a value of a type parameter whose `where` bounds declare no
    /// method of that name (`parameter` names the type parameter, `method` the called name).
    GenericReceiverMethodUndeclared {
        method: String,
        parameter: String,
    },
    /// E1607: a `[CompilerDerived]` contract listed in a conformance list (`type X : C`,
    /// `impl X : C`); conformance to it is proven only by the transfer class query.
    CompilerDerivedConformance {
        contract: String,
    },
    /// E1607: `[CompilerDerived]` on a contract that is not one of the four marker contracts.
    CompilerDerivedNotMarker {
        contract: String,
    },
    /// E1003: a member declared in a `[CompilerDerived]` contract.
    DerivedContractMember {
        contract: String,
        member: String,
    },
    /// E1510: a second marker among `[Immutable]`, `[Isolated]` and `[Copyable]` on one type.
    ExclusiveMarkerAttribute {
        marker: String,
        previous: String,
    },
    /// E1512: a `[CopyOnAssign]` type without a valid `This Copy()` hook; `problem` says what is
    /// wrong ("it declares no `Copy` method").
    CopyOnAssignWithoutCopy {
        type_name: String,
        problem: String,
    },
    /// E1510: an attribute that only corelib (`Core.*` modules) may use, such as `[ImmutableStorage]`.
    CorelibOnlyAttribute {
        attribute: String,
    },
    /// Internal compiler error E2101 (E2101-E2199 band): a semantic fact a lowering request needs
    /// is still unavailable after the legality gate passed for its items. A compiler gap, never a
    /// user error. `construct` is the syntax node kind of the site (`AssignExpression`).
    InternalSemanticFactUnavailable {
        query: String,
        construct: String,
    },
    /// Internal compiler error E2102: no ISLE lowering rule or fact exists for a construct after
    /// the legality gate passed for its item.
    InternalLoweringRuleMissing {
        construct: String,
    },
    /// Internal compiler error E2103: lowering or module emission met an inconsistency (an
    /// invalid layout, a failed verification). `reason` is a readable sentence, never debug text.
    InternalLoweringInvariant {
        construct: String,
        reason: String,
    },
    /// Internal compiler error E2104: the compiler could not answer a Mod's semantic request.
    InternalModSemanticFailure {
        reason: String,
    },
    /// Internal compiler error E2105: a build step failed on a compiler inconsistency that has no
    /// source site (a stringified upstream failure).
    InternalBuildFailure {
        reason: String,
    },
    TypeInvalidEventInvocationScope,
    TypeInvalidEventCapacity,
    TypeInvalidEventSubscriptionTarget,
    SpawnTargetNotFiberCompatible,
    JoinWouldDeadlock,
    StackReferenceEscapesSpawn,
    AsyncKeywordReserved,
    AwaitKeywordReserved,

    // ── Control-flow / match diagnostics ──
    TypeReturnMismatch {
        expected_name: String,
        actual_name: String,
    },
    TypeNonIterableForTarget,
    TypeIterableNextArityMismatch {
        expected: usize,
        actual: usize,
    },
    TypeIterableNextReturnNotOption,
    TypeIterableOptionSomeArityMismatch {
        expected: usize,
        actual: usize,
    },
    TypeImplicitNumericCast {
        from: String,
        to: String,
    },

    // ── Macro diagnostics ──
    MacroUnknown {
        name: String,
    },
    MacroArgumentArityMismatch {
        name: String,
        expected: usize,
        actual: usize,
    },
    MacroArgumentKindMismatch {
        name: String,
        parameter: String,
        expected_kind: String,
    },
    MacroMetavariableOutsideBody {
        name: String,
    },
    MacroExpansionDepthExceeded {
        max_depth: u32,
    },
    MacroAmbiguousName {
        name: String,
    },
    MacroDuplicateParameter {
        name: String,
        parameter: String,
    },
    QueryBoundsExceeded {
        max_nodes: u64,
        max_depth: u64,
    },
    QueryNodeSpanUnavailable,
    QueryPipelineConflict,
    QueryPipelineStaleGeneration,

    // ── Documentation diagnostics ──
    /// `@arg(name)` does not match a parameter on the documented callable.
    DocUnknownArgName {
        name: String,
    },
    /// Duplicate `@arg(name)` in the same documentation block.
    DocDuplicateArgName {
        name: String,
    },
    /// `@arg` / `@returns` used where only callables accept them.
    DocArgOrReturnsOnNonCallable,
    /// `@returns` on a `unit` return type.
    DocReturnsOnUnit,
    /// Unknown `@foo` documentation directive.
    DocUnknownDirective {
        name: String,
    },
    /// `@ref(...)` path does not resolve.
    DocUnresolvedRef {
        path: String,
    },
    /// `@variant(...)` is only valid on an enum declaration's leading documentation.
    DocVariantOnNonEnum,
    /// `@variant(name)` does not match any variant on this enum.
    DocUnknownVariantName {
        name: String,
    },
    /// Duplicate `@variant(name)` in the same documentation block.
    DocDuplicateVariantName {
        name: String,
    },
    /// `@par(...)` requires generic type parameters on this declaration.
    DocParWithoutGenerics,
    /// `@par(name)` does not match any generic type parameter on this item.
    DocUnknownGenericName {
        name: String,
    },
    /// Duplicate `@par(name)` in the same documentation block.
    DocDuplicateGenericName {
        name: String,
    },
    RedundantEnumConstructorParens,

    // ── Naming-style warnings (W1630–W1638) ──
    NamingNotPascalCaseType {
        name: String,
    },
    NamingNotPascalCaseVariant {
        name: String,
    },
    NamingNotCamelCaseField {
        name: String,
    },
    NamingNotPascalCaseCallable {
        name: String,
    },
    NamingNotPascalCaseModuleSegment {
        segment: String,
    },
    NamingNotPascalCaseGeneric {
        name: String,
    },
    NamingNotCamelCaseBinding {
        name: String,
    },
    NamingNotPascalCaseTest {
        name: String,
    },
    NamingNotCamelCaseMacro {
        name: String,
    },

    // ── Composition diagnostics ──
    CompositionMissingLaunchHost,
    CompositionMultipleLaunchHosts,
    CompositionDependencyCycle {
        from_id: u32,
        to_id: u32,
    },
    CompositionUnresolvedInject {
        requested_type: String,
    },
    CompositionAmbiguousInject {
        requested_type: String,
    },
    CompositionScopedOutsideWith,
    CompositionChildScopeWithoutParent {
        scope_name: String,
    },
    CompositionWithArgsMismatch {
        scope_name: String,
    },
    CompositionLaunchTargetNotHost {
        target_name: String,
    },
    CompositionHostInheritanceCycle {
        host_name: String,
    },
    CompositionDuplicateScopeName {
        scope_name: String,
    },
    CompositionHostInModProject,
    CompositionLaunchInLibProject,
    CompositionInjectOnConstructor,
    CompositionOverrideLifetimeMismatch {
        binding: String,
    },
    CompositionInvalidScopeQualifier {
        qualifier: String,
    },

    // ── Query-gate numeric literal, extern profile, and Glue obligations ──
    /// T0905: a numeric literal (`value` as written) or integer constant use (`value` is its
    /// name) that does not fit `primitive`, the type its suffix or its context gives it.
    NumericLiteralOutOfRange {
        value: String,
        primitive: String,
    },
    /// T0901: an `[Extern]` contract whose `Abi` is not `"C"`.
    ExternInvalidAbi {
        abi: Option<String>,
    },
    /// T0902: an `[Extern]` contract without a `Library`.
    ExternMissingLibrary,
    /// T0903: an extern method parameter the C profile (or the Glue profile) rejects.
    ExternDisallowedParamType {
        method: String,
        detail: String,
    },
    /// T0904: an extern method return type the C profile rejects.
    ExternDisallowedReturnType {
        method: String,
        detail: String,
    },
    /// T0903: an `[Extern]` method of a manifest Glue library whose signature or `RustOwner`
    /// mapping the Glue binding authority rejects.
    GlueBindingRejected {
        method: String,
        detail: String,
    },
    /// T0906: an `[Extern]` contract whose `Optional` argument is not a boolean literal, or an
    /// `Optional:true` contract whose availability query is not declared `bool Available();`.
    /// `detail` names the violated rule.
    ExternInvalidOptional {
        detail: String,
    },
    /// E1232: a `clif { ... }` block without a typed scalar context, with a body the CLIF
    /// surface rejects, or naming a `%N` parameter it may not use. `detail` names the rule.
    InvalidClifBlock {
        detail: String,
    },
}

/// The declaration of a binding an E1214 assignment writes, as the renderer needs it to tailor
/// the help and the fix to the declaration form (`mut i32 count = 0;`, `let mut count = 0;`,
/// `mut i32 count` for a parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ImmutableBinding {
    pub form: BindingForm,
    /// The whole declaration (`i32 count = 0;`, `let count = 0;`, `i32 count`).
    pub declaration: SpanInfo,
    /// Where `mut ` is inserted: the type of a typed local or parameter, the name of a `let`.
    pub mut_at: usize,
}

/// How a discarded `Array.Append` (W1240) is repaired.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DiscardedGrowthRepair {
    /// The first argument is a parameter that is not `mut`: declaring it `mut` makes the call grow
    /// the caller's array in place. `insert_at` is the byte offset of the parameter declaration.
    MutParameter { name: String, insert_at: usize },
    /// The first argument is an assignable name: assigning the result keeps the growth.
    /// `insert_at` is the byte offset of the call.
    Assign { name: String, insert_at: usize },
    /// No name to assign (a loop variable, a call result, an element): bind the result to a new
    /// local. `declaration` is the written type when the call names its element type.
    Bind { declaration: Option<String>, insert_at: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BindingForm {
    TypedLocal,
    InferredLocal,
    Parameter,
    ForIterator,
}

impl ImmutableBinding {
    /// The binding of a `let` statement spanning `declaration`.
    pub fn of_let(statement: &crate::syntax::LetStatement, declaration: SpanInfo) -> Self {
        match &statement.type_annotation {
            Some(ty) => Self { form: BindingForm::TypedLocal, declaration, mut_at: ty.span.start },
            None => Self { form: BindingForm::InferredLocal, declaration, mut_at: statement.name.span.start },
        }
    }

    /// The binding of a parameter.
    pub fn of_parameter(parameter: &crate::syntax::Spanned<crate::syntax::Parameter>) -> Self {
        Self { form: BindingForm::Parameter, declaration: parameter.span, mut_at: parameter.node.ty.span.start }
    }

    /// The binding of a `for` iterator (`name` is the iterator identifier).
    pub fn of_for_iterator(name: SpanInfo) -> Self {
        Self { form: BindingForm::ForIterator, declaration: name, mut_at: name.start }
    }
}

#[cfg(test)]
mod query_gate_kind_tests {
    use super::SemanticIssueKind;
    use crate::analysis::Severity;

    #[test]
    fn extern_invalid_optional_has_its_own_code_and_carries_the_rule_as_help() {
        let issue = SemanticIssueKind::ExternInvalidOptional {
            detail: "Optional takes a boolean literal: Optional:true or Optional:false".to_string(),
        };
        assert_eq!(issue.code(), "T0906");
        assert_ne!(issue.code(), SemanticIssueKind::NumericLiteralOutOfRange { value: "300".to_string(), primitive: "i8".to_string() }.code());
        assert_eq!(issue.message(), "invalid optional extern contract");
        assert_eq!(issue.label(), "extern attribute");
        assert_eq!(issue.help().as_deref(), Some("Optional takes a boolean literal: Optional:true or Optional:false"));
        assert_eq!(issue.severity(), Severity::Error);
    }

    #[test]
    fn invalid_clif_block_renders_the_legacy_code_and_carries_the_rule_as_help() {
        let issue = SemanticIssueKind::InvalidClifBlock {
            detail: "clif blocks inside lambdas cannot name parameters with `%N`".to_string(),
        };
        assert_eq!(issue.code(), "E1232");
        assert_eq!(issue.message(), "invalid clif block");
        assert_eq!(issue.label(), "clif block");
        assert_eq!(issue.help().as_deref(), Some("clif blocks inside lambdas cannot name parameters with `%N`"));
        assert_eq!(issue.severity(), Severity::Error);
    }

    #[test]
    fn transfer_capture_and_move_block_maps_e1234_through_e1249_in_registry_order() {
        let name = || "value".to_string();
        let kinds = [
            SemanticIssueKind::UseAfterMove { name: name() },
            SemanticIssueKind::CapturedLocalAssignedInBoundLambda { name: name() },
            SemanticIssueKind::CaptureNotTransferable { name: name(), type_name: "Channel<i64>".into(), class: "Never".into() },
            SemanticIssueKind::MovedCaptureWithoutMove { name: name() },
            SemanticIssueKind::CapturesArgumentNotLiteral,
            SemanticIssueKind::MoveOfNonOwningName { name: name(), class: Some("Shared".into()) },
            SemanticIssueKind::MoveInLoopWithoutReassignment { name: name() },
            SemanticIssueKind::BorrowEscapes { name: name() },
            SemanticIssueKind::ImplicitAliasOfMovedValue { name: name() },
            SemanticIssueKind::DestructiveReadWithoutEmpty { field: "f".into(), type_name: "i64".into() },
            SemanticIssueKind::ImmutableFieldNotShared { type_name: "T".into(), field: "f".into(), class: "Moved".into() },
            SemanticIssueKind::ImmutableFieldAssignment { type_name: "T".into(), field: "f".into() },
            SemanticIssueKind::MarkerOnNeverTransferable {
                marker: "Isolated".into(),
                type_name: "T".into(),
                cause: "`[NeverTransferable]`".into(),
            },
            SemanticIssueKind::MarkerFieldClassMismatch {
                marker: "Isolated".into(),
                type_name: "T".into(),
                field: "f".into(),
                class: "Copied".into(),
            },
            SemanticIssueKind::CapturesBoundNotMarker { bound: None },
            SemanticIssueKind::InjectInsideIsolateBody { field: "clock".into() },
        ];
        for (offset, kind) in kinds.iter().enumerate() {
            assert_eq!(kind.code(), format!("E{}", 1234 + offset), "{kind:?}");
            assert_eq!(kind.severity(), Severity::Error);
            assert!(!kind.message().is_empty() && !kind.label().is_empty());
        }
        assert_eq!(
            SemanticIssueKind::MovedCaptureWithoutMove { name: "item".into() }.help().as_deref(),
            Some("write `move item`")
        );
        assert_eq!(SemanticIssueKind::CapturedLocalAssignedInBoundLambda { name: name() }.help().as_deref(), Some("use a reduction"));
        assert_eq!(SemanticIssueKind::CapturesArgumentNotLiteral.help().as_deref(), Some("pass the lambda literal"));
        assert_eq!(
            SemanticIssueKind::BorrowEscapes { name: "buffer".into() }.help().as_deref(),
            Some("copy `buffer` first: another name in this callable can still reach its value")
        );
        assert_eq!(
            SemanticIssueKind::BorrowEscapes { name: "holder.bytes".into() }.help().as_deref(),
            Some("copy `holder.bytes` first, or take it out with `move holder.bytes`")
        );
        assert_eq!(
            SemanticIssueKind::BorrowEscapes { name: "chunks[i]".into() }.help().as_deref(),
            Some("copy `chunks[i]` first, or take it out with `exchange(chunks[i], <value>)`")
        );
        assert_eq!(
            SemanticIssueKind::ImplicitAliasOfMovedValue { name: "block".into() }.help().as_deref(),
            Some("write `move block`")
        );
        assert_eq!(
            SemanticIssueKind::DestructiveReadWithoutEmpty { field: "f".into(), type_name: "i64".into() }.help().as_deref(),
            Some("use `exchange(<place>, <value>)`")
        );
        assert_eq!(
            SemanticIssueKind::InjectInsideIsolateBody { field: "clock".into() }.message(),
            "composition is not available inside an isolate"
        );
    }
}
