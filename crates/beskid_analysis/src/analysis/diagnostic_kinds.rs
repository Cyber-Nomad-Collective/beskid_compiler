mod code;
mod help;
mod label;
mod message;
mod severity;

use crate::syntax::SpanInfo;

/// Package-native replacement for a module path that starts with the removed `Std` segment.
///
/// Beskid has no `Std` namespace: Corelib packages keep their own module paths (`Core.*`,
/// `Testing.*`, `Concurrency.*`, `Beskid.Compiler.*`). A `Std.`-qualified path is therefore the
/// native path with one extra leading segment. Returns `None` for paths that do not start with
/// `Std`. The replacement keeps the separator style of `path` (`.` or `::`).
pub fn removed_std_namespace_replacement(path: &str) -> Option<String> {
    let rest = if let Some(rest) = path.strip_prefix("Std.") {
        rest
    } else if let Some(rest) = path.strip_prefix("Std::") {
        rest
    } else if path == "Std" {
        ""
    } else {
        return None;
    };
    Some(if rest.is_empty() { "Core".to_string() } else { rest.to_string() })
}

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
    ResolvePrivateRuntimeBuiltin {
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
    TypeUnknownStructField {
        name: String,
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
    TypeMissingTypeArguments,
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
    /// Internal compiler error (E2101-E2199 band): a semantic fact a lowering request needs is
    /// still unavailable after the legality gate passed for its items. A compiler gap, never a
    /// user error.
    InternalSemanticFactUnavailable {
        query: String,
    },
    /// Internal compiler error: no ISLE lowering rule or fact exists for a construct after the
    /// legality gate passed for its item. A compiler gap, never a user error.
    InternalLoweringRuleMissing {
        construct: String,
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

    // ── Query-gate numeric literal, extern profile, and Glue obligations ──
    /// T0905: a suffixed integer literal, or a float literal, outside its declared fixed-width
    /// primitive's range.
    NumericLiteralOutOfRange {
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
}

#[cfg(test)]
mod removed_std_namespace_tests {
    use super::{SemanticIssueKind, removed_std_namespace_replacement};

    #[test]
    fn std_prefixed_paths_name_their_package_native_replacement() {
        assert_eq!(removed_std_namespace_replacement("Std.Core.Output").as_deref(), Some("Core.Output"));
        assert_eq!(removed_std_namespace_replacement("Std::Testing::Assert").as_deref(), Some("Testing::Assert"));
        assert_eq!(removed_std_namespace_replacement("Std").as_deref(), Some("Core"));
        assert_eq!(removed_std_namespace_replacement("Core.Output"), None);
        assert_eq!(removed_std_namespace_replacement("Stdio.Bridge"), None);
    }

    #[test]
    fn std_import_diagnostic_names_the_replacement() {
        let issue = SemanticIssueKind::UnknownImportPath { path: "Std.Core.Output".to_string() };
        assert_eq!(issue.code(), "E1105");
        assert!(issue.message().contains("the `Std` namespace does not exist"), "{}", issue.message());
        let help = issue.help().expect("help");
        assert!(help.starts_with("use `Core.Output` instead"), "{help}");

        let module = SemanticIssueKind::ResolveUnknownModulePath { path: "Std::Core::Results".to_string() };
        assert!(module.help().expect("help").starts_with("use `Core::Results` instead"));
    }
}
