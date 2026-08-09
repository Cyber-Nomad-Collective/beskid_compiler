mod code;
mod help;
mod label;
mod message;
mod severity;

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
    },
    UseBeforeDeclaration {
        name: String,
    },
    InvalidHirSpan {
        context: String,
    },
    UnresolvedHirValuePath,
    UnresolvedHirTypePath,
    NonNormalizedHirControlFlow {
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
    AttributeTargetNotAllowed {
        attribute: String,
        target: String,
        allowed: Vec<String>,
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
    UnusedPrivateItem {
        name: String,
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

    ImmutableAssignment {
        name: String,
    },

    // ── Pattern / match / control-flow diagnostics ──
    MatchGuardMustBeBoolean,
    MatchArmTypeMismatch {
        expected: String,
        actual: String,
    },
    MatchNonExhaustive {
        enum_name: String,
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
    ResolvePrivateItemInModule {
        module_path: String,
        name: String,
    },
    ResolveShadowedLocal {
        name: String,
        previous: SpanInfo,
    },

    // ── Type-checking diagnostics ──
    TypeUnknownType {
        name: String,
    },
    TypeUnknownValueType,
    TypeUnknownStructType,
    TypeInvalidMemberTarget,
    TypeUnknownEnumType,
    TypeUnknownStructField {
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
    TypeMissingTypeArguments,
    TypeGenericArgumentMismatch {
        expected: usize,
        actual: usize,
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
    TypeInvalidUnaryOp,
    TypeNonBoolCondition,
    TypeUnsupportedExpression,
    TypeInvalidTryTarget,
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
    NamingNotSnakeCaseTest {
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
}
