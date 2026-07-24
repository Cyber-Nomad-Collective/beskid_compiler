use crate::analysis::Severity;
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

impl SemanticIssueKind {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicateDefinitionName { .. } => "E1001",
            Self::DuplicateEnumVariant { .. } => "E1002",
            Self::DuplicateContractMethod { .. } => "E1003",
            Self::ConflictingEmbeddedContractMethod { .. } => "E1004",
            Self::UnknownTypeInDefinition { .. } => "E1005",
            Self::DuplicateItemName { .. } => "E1006",

            Self::AmbiguousImport { .. } => "E1104",
            Self::UnknownImportPath { .. } => "E1105",
            Self::UseBeforeDeclaration { .. } => "E1106",
            Self::InvalidHirSpan { .. } => "E1151",
            Self::UnresolvedHirValuePath => "E1152",
            Self::UnresolvedHirTypePath => "E1153",
            Self::NonNormalizedHirControlFlow { .. } => "E1154",
            Self::DuplicateAttributeDeclarationTarget { .. } => "E1508",
            Self::UnknownAttributeDeclarationTarget { .. } => "E1509",
            Self::AttributeTargetNotAllowed { .. } => "E1510",

            Self::VisibilityViolationImportPrivate { .. } => "E1501",
            Self::VisibilityModuleNotFound { .. } => "E1502",
            Self::FileScopedModuleNotFirstItem { .. } => "E1505",
            Self::DuplicateFileScopedModule { .. } => "E1506",
            Self::ModuleDeclarationForbiddenInFileScopedModule => "E1507",
            Self::ExtendTypePrivateMemberAccess { .. } => "E1511",
            Self::UnusedImport { .. } => "W1503",
            Self::UnusedPrivateItem { .. } => "W1504",

            Self::ContractMethodMissingImplementation { .. } => "E1601",
            Self::ContractImplementationSignatureMismatch { .. } => "E1602",
            Self::ContractMethodNotFound { .. } => "E1606",

            Self::ImmutableAssignment { .. } => "E1214",

            Self::MatchGuardMustBeBoolean => "E1308",
            Self::MatchArmTypeMismatch { .. } => "E1305",
            Self::MatchNonExhaustive { .. } => "E1304",
            Self::DuplicatePatternBinding { .. } => "E1306",
            Self::UnknownEnumPath { .. } => "E1301",
            Self::PatternArityMismatch { .. } => "E1307",
            Self::EnumConstructorArityMismatch { .. } => "E1302",
            Self::UnqualifiedEnumConstructor { .. } => "E1303",
            Self::BreakOutsideLoop => "E1401",
            Self::ContinueOutsideLoop => "E1402",
            Self::UnreachableCode => "W1403",

            Self::ResolveDuplicateItem { .. } => "E1102",
            Self::ResolveDuplicateLocal { .. } => "E1102",
            Self::ResolveUnknownValue { .. } => "E1101",
            Self::ResolveUnknownType { .. } => "E1201",
            Self::ResolveUnknownModulePath { .. } => "E1108",
            Self::ResolveUnknownValueInModule { .. } => "E1101",
            Self::ResolveUnknownTypeInModule { .. } => "E1201",
            Self::ResolveInvalidConformanceTarget { .. } => "E1607",
            Self::ResolvePrivateItemInModule { .. } => "E1107",
            Self::ResolveShadowedLocal { .. } => "W1103",

            Self::TypeUnknownType { .. } => "E1201",
            Self::TypeUnknownValueType => "E1201",
            Self::TypeUnknownStructType => "E1201",
            Self::TypeInvalidMemberTarget => "E1213",
            Self::TypeUnknownEnumType => "E1201",
            Self::TypeUnknownStructField { .. } => "E1211",
            Self::TypeUnknownEnumVariant { .. } => "E1301",
            Self::TypeMissingStructField { .. } => "E1212",
            Self::TypeMissingTypeAnnotation { .. } => "E1202",
            Self::TypeMissingTypeArguments => "E1203",
            Self::TypeGenericArgumentMismatch { .. } => "E1204",
            Self::TypeMismatch { .. } => "E1206",
            Self::TypeMatchArmMismatch { .. } => "E1305",
            Self::TypeCallArityMismatch { .. } => "E1204",
            Self::TypeCallArgumentMismatch { .. } => "E1205",
            Self::TypeEnumConstructorMismatch { .. } => "E1302",
            Self::TypeUnknownCallTarget => "E1606",
            Self::TypeInvalidBinaryOp => "E1209",
            Self::TypeInvalidUnaryOp => "E1210",
            Self::TypeNonBoolCondition => "E1208",
            Self::TypeUnsupportedExpression => "E1202",
            Self::TypeInvalidTryTarget => "E1222",
            Self::TypeInvalidEventInvocationScope => "E1219",
            Self::TypeInvalidEventCapacity => "E1220",
            Self::TypeInvalidEventSubscriptionTarget => "E1221",
            Self::SpawnTargetNotFiberCompatible => "E1223",
            Self::JoinWouldDeadlock => "E1224",
            Self::StackReferenceEscapesSpawn => "E1225",
            Self::AsyncKeywordReserved => "E1226",
            Self::AwaitKeywordReserved => "E1227",
            Self::TypeReturnMismatch { .. } => "E1207",
            Self::TypeNonIterableForTarget => "E1215",
            Self::TypeIterableNextArityMismatch { .. } => "E1216",
            Self::TypeIterableNextReturnNotOption => "E1217",
            Self::TypeIterableOptionSomeArityMismatch { .. } => "E1218",
            Self::TypeImplicitNumericCast { .. } => "W1203",

            Self::MacroUnknown { .. } => "E1901",
            Self::MacroArgumentArityMismatch { .. } => "E1902",
            Self::MacroArgumentKindMismatch { .. } => "E1903",
            Self::MacroMetavariableOutsideBody { .. } => "E1904",
            Self::MacroExpansionDepthExceeded { .. } => "E1905",
            Self::MacroAmbiguousName { .. } => "E1907",
            Self::MacroDuplicateParameter { .. } => "E1908",
            Self::QueryBoundsExceeded { .. } => "E1880",
            Self::QueryNodeSpanUnavailable => "E1881",
            Self::QueryPipelineConflict => "E1883",
            Self::QueryPipelineStaleGeneration => "E1884",

            Self::CompositionMissingLaunchHost => "E1701",
            Self::CompositionMultipleLaunchHosts => "E1702",
            Self::CompositionDependencyCycle { .. } => "E1703",
            Self::CompositionUnresolvedInject { .. } => "E1704",
            Self::CompositionAmbiguousInject { .. } => "E1705",
            Self::CompositionScopedOutsideWith => "E1706",
            Self::CompositionChildScopeWithoutParent { .. } => "E1707",
            Self::CompositionWithArgsMismatch { .. } => "E1708",
            Self::CompositionLaunchTargetNotHost { .. } => "E1709",
            Self::CompositionHostInheritanceCycle { .. } => "E1715",
            Self::CompositionDuplicateScopeName { .. } => "E1716",
            Self::CompositionHostInModProject => "E1710",
            Self::CompositionLaunchInLibProject => "E1711",
            Self::CompositionInjectOnConstructor => "E1712",
            Self::CompositionOverrideLifetimeMismatch { .. } => "E1713",
            Self::CompositionInvalidScopeQualifier { .. } => "E1714",

            Self::DocUnknownArgName { .. } => "W1610",
            Self::DocDuplicateArgName { .. } => "W1611",
            Self::DocArgOrReturnsOnNonCallable => "W1612",
            Self::DocReturnsOnUnit => "W1613",
            Self::DocUnknownDirective { .. } => "W1614",
            Self::DocUnresolvedRef { .. } => "W1615",
            Self::DocVariantOnNonEnum => "W1620",
            Self::DocUnknownVariantName { .. } => "W1621",
            Self::DocDuplicateVariantName { .. } => "W1622",
            Self::DocParWithoutGenerics => "W1623",
            Self::DocUnknownGenericName { .. } => "W1624",
            Self::DocDuplicateGenericName { .. } => "W1625",
            Self::RedundantEnumConstructorParens => "W1639",

            Self::NamingNotPascalCaseType { .. } => "W1630",
            Self::NamingNotPascalCaseVariant { .. } => "W1631",
            Self::NamingNotCamelCaseField { .. } => "W1632",
            Self::NamingNotPascalCaseCallable { .. } => "W1633",
            Self::NamingNotPascalCaseModuleSegment { .. } => "W1634",
            Self::NamingNotPascalCaseGeneric { .. } => "W1635",
            Self::NamingNotCamelCaseBinding { .. } => "W1636",
            Self::NamingNotSnakeCaseTest { .. } => "W1637",
            Self::NamingNotCamelCaseMacro { .. } => "W1638",
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            Self::UnusedImport { .. }
            | Self::UnusedPrivateItem { .. }
            | Self::UnreachableCode
            | Self::ResolveShadowedLocal { .. }
            | Self::TypeImplicitNumericCast { .. }
            | Self::DocUnknownArgName { .. }
            | Self::DocDuplicateArgName { .. }
            | Self::DocArgOrReturnsOnNonCallable
            | Self::DocReturnsOnUnit
            | Self::DocUnknownDirective { .. }
            | Self::DocUnresolvedRef { .. }
            | Self::DocVariantOnNonEnum
            | Self::DocUnknownVariantName { .. }
            | Self::DocDuplicateVariantName { .. }
            | Self::DocParWithoutGenerics
            | Self::DocUnknownGenericName { .. }
            | Self::DocDuplicateGenericName { .. }
            | Self::NamingNotPascalCaseType { .. }
            | Self::NamingNotPascalCaseVariant { .. }
            | Self::NamingNotCamelCaseField { .. }
            | Self::NamingNotPascalCaseCallable { .. }
            | Self::NamingNotPascalCaseModuleSegment { .. }
            | Self::NamingNotPascalCaseGeneric { .. }
            | Self::NamingNotCamelCaseBinding { .. }
            | Self::NamingNotSnakeCaseTest { .. }
            | Self::NamingNotCamelCaseMacro { .. }
            | Self::RedundantEnumConstructorParens => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::DuplicateDefinitionName { .. } => "duplicate definition name".to_string(),
            Self::DuplicateEnumVariant { .. } => "duplicate enum variant".to_string(),
            Self::DuplicateContractMethod { .. } => "duplicate contract method".to_string(),
            Self::DuplicateItemName { .. } => "duplicate item name".to_string(),
            Self::UnknownTypeInDefinition { .. } => "unknown type in definition".to_string(),
            Self::ConflictingEmbeddedContractMethod { .. } => "conflicting embedded contract method".to_string(),
            Self::AmbiguousImport { .. } => "ambiguous import".to_string(),
            Self::UnknownImportPath { .. } => "unknown import path".to_string(),
            Self::UseBeforeDeclaration { .. } => "use before declaration".to_string(),
            Self::InvalidHirSpan { .. } => "invalid HIR span".to_string(),
            Self::UnresolvedHirValuePath => "unresolved HIR value path".to_string(),
            Self::UnresolvedHirTypePath => "unresolved HIR type path".to_string(),
            Self::NonNormalizedHirControlFlow { .. } => "non-normalized HIR control-flow".to_string(),
            Self::DuplicateAttributeDeclarationTarget { .. } => "duplicate attribute declaration target".to_string(),
            Self::UnknownAttributeDeclarationTarget { .. } => "unknown attribute declaration target".to_string(),
            Self::AttributeTargetNotAllowed { .. } => "attribute target not allowed".to_string(),
            Self::VisibilityModuleNotFound { .. } => "module not found".to_string(),
            Self::FileScopedModuleNotFirstItem { .. } => "file-scoped module must be first item".to_string(),
            Self::DuplicateFileScopedModule { .. } => "duplicate file-scoped module".to_string(),
            Self::ModuleDeclarationForbiddenInFileScopedModule => "module declaration not allowed".to_string(),
            Self::VisibilityViolationImportPrivate { .. } => "visibility violation".to_string(),
            Self::ExtendTypePrivateMemberAccess { .. } => "extend type private member access".to_string(),
            Self::UnusedImport { .. } => "unused import".to_string(),
            Self::UnusedPrivateItem { .. } => "unused private item".to_string(),
            Self::ContractMethodNotFound { .. } => "method not found".to_string(),
            Self::ContractImplementationSignatureMismatch { .. } => {
                "contract implementation signature mismatch".to_string()
            }
            Self::ContractMethodMissingImplementation { .. } => "contract method missing implementation".to_string(),
            Self::ImmutableAssignment { .. } => "immutable assignment".to_string(),
            Self::MatchGuardMustBeBoolean => "guard type mismatch".to_string(),
            Self::MatchArmTypeMismatch { .. } => "match arm type mismatch".to_string(),
            Self::MatchNonExhaustive { .. } => "match non-exhaustive".to_string(),
            Self::DuplicatePatternBinding { .. } => "duplicate pattern binding".to_string(),
            Self::UnknownEnumPath { .. } => "unknown enum path".to_string(),
            Self::PatternArityMismatch { .. } => "pattern arity mismatch".to_string(),
            Self::EnumConstructorArityMismatch { .. } => "enum constructor arity mismatch".to_string(),
            Self::UnqualifiedEnumConstructor { .. } => "unqualified enum constructor".to_string(),
            Self::BreakOutsideLoop => "break outside loop".to_string(),
            Self::ContinueOutsideLoop => "continue outside loop".to_string(),
            Self::UnreachableCode => "unreachable statement".to_string(),
            Self::ResolveDuplicateItem { .. } => "duplicate item".to_string(),
            Self::ResolveDuplicateLocal { .. } => "duplicate local".to_string(),
            Self::ResolveUnknownValue { .. } => "unknown value".to_string(),
            Self::ResolveUnknownType { .. } => "unknown type".to_string(),
            Self::ResolveUnknownModulePath { .. } => "unknown module path".to_string(),
            Self::ResolveUnknownValueInModule { .. } => "unknown value in module".to_string(),
            Self::ResolveUnknownTypeInModule { .. } => "unknown type in module".to_string(),
            Self::ResolveInvalidConformanceTarget { .. } => "invalid conformance target".to_string(),
            Self::ResolvePrivateItemInModule { .. } => "private item access".to_string(),
            Self::ResolveShadowedLocal { .. } => "shadowed local".to_string(),
            Self::TypeUnknownType { .. } => "unknown type".to_string(),
            Self::TypeUnknownValueType => "unknown value type".to_string(),
            Self::TypeUnknownStructType => "unknown struct type".to_string(),
            Self::TypeInvalidMemberTarget => "invalid member access target".to_string(),
            Self::TypeUnknownEnumType => "unknown enum type".to_string(),
            Self::TypeUnknownStructField { .. } => "unknown struct field".to_string(),
            Self::TypeUnknownEnumVariant { .. } => "unknown enum variant".to_string(),
            Self::TypeMissingStructField { .. } => "missing struct field".to_string(),
            Self::TypeMissingTypeAnnotation { .. } => "missing type annotation".to_string(),
            Self::TypeMissingTypeArguments => "missing type arguments".to_string(),
            Self::TypeGenericArgumentMismatch { .. } => "generic argument mismatch".to_string(),
            Self::TypeMismatch { .. } => "type mismatch".to_string(),
            Self::TypeMatchArmMismatch { .. } => "match arm type mismatch".to_string(),
            Self::TypeCallArityMismatch { .. } => "call arity mismatch".to_string(),
            Self::TypeCallArgumentMismatch { .. } => "call argument mismatch".to_string(),
            Self::TypeEnumConstructorMismatch { .. } => "enum constructor arity mismatch".to_string(),
            Self::TypeUnknownCallTarget => "unknown call target".to_string(),
            Self::TypeInvalidBinaryOp => "invalid binary operation".to_string(),
            Self::TypeInvalidUnaryOp => "invalid unary operation".to_string(),
            Self::TypeNonBoolCondition => "condition must be boolean".to_string(),
            Self::TypeUnsupportedExpression => "unsupported expression".to_string(),
            Self::TypeInvalidTryTarget => "invalid try target".to_string(),
            Self::TypeInvalidEventInvocationScope => "invalid event invocation scope".to_string(),
            Self::TypeInvalidEventCapacity => "invalid event capacity".to_string(),
            Self::TypeInvalidEventSubscriptionTarget => "invalid event subscription target".to_string(),
            Self::SpawnTargetNotFiberCompatible => "spawn target not fiber compatible".to_string(),
            Self::JoinWouldDeadlock => "join would deadlock".to_string(),
            Self::StackReferenceEscapesSpawn => "stack reference escapes spawn".to_string(),
            Self::AsyncKeywordReserved => "async keyword reserved".to_string(),
            Self::AwaitKeywordReserved => "await keyword reserved".to_string(),
            Self::MacroUnknown { .. } => "unknown macro".to_string(),
            Self::MacroArgumentArityMismatch { .. } => "macro argument count mismatch".to_string(),
            Self::MacroArgumentKindMismatch { .. } => "macro argument kind mismatch".to_string(),
            Self::MacroMetavariableOutsideBody { .. } => "macro metavariable outside body".to_string(),
            Self::MacroExpansionDepthExceeded { .. } => "macro expansion depth exceeded".to_string(),
            Self::MacroAmbiguousName { .. } => "ambiguous macro name".to_string(),
            Self::MacroDuplicateParameter { .. } => "duplicate macro parameter".to_string(),
            Self::QueryBoundsExceeded { .. } => "query bounds exceeded".to_string(),
            Self::QueryNodeSpanUnavailable => "node span unavailable".to_string(),
            Self::QueryPipelineConflict => "query pipeline conflict".to_string(),
            Self::QueryPipelineStaleGeneration => "query pipeline stale generation".to_string(),

            Self::CompositionMissingLaunchHost => "missing launch host".to_string(),
            Self::CompositionMultipleLaunchHosts => "multiple launch hosts".to_string(),
            Self::CompositionDependencyCycle { .. } => "composition dependency cycle".to_string(),
            Self::CompositionUnresolvedInject { .. } => "unresolved inject".to_string(),
            Self::CompositionAmbiguousInject { .. } => "ambiguous inject".to_string(),
            Self::CompositionScopedOutsideWith => "scope used outside with".to_string(),
            Self::CompositionChildScopeWithoutParent { .. } => "child scope without parent".to_string(),
            Self::CompositionWithArgsMismatch { .. } => "with argument mismatch".to_string(),
            Self::CompositionLaunchTargetNotHost { .. } => "launch target is not a host".to_string(),
            Self::CompositionHostInheritanceCycle { .. } => "host inheritance cycle".to_string(),
            Self::CompositionDuplicateScopeName { .. } => "duplicate scope name".to_string(),
            Self::CompositionHostInModProject => "host in mod project".to_string(),
            Self::CompositionLaunchInLibProject => "launch in lib project".to_string(),
            Self::CompositionInjectOnConstructor => "inject on constructor".to_string(),
            Self::CompositionOverrideLifetimeMismatch { .. } => "override lifetime mismatch".to_string(),
            Self::CompositionInvalidScopeQualifier { .. } => "invalid scope qualifier".to_string(),

            Self::TypeReturnMismatch { .. } => "return type mismatch".to_string(),
            Self::TypeNonIterableForTarget => "non-iterable for target".to_string(),
            Self::TypeIterableNextArityMismatch { .. } => "iterable Next arity mismatch".to_string(),
            Self::TypeIterableNextReturnNotOption => "iterable Next return must be Option<T>".to_string(),
            Self::TypeIterableOptionSomeArityMismatch { .. } => "iterable Option::Some arity mismatch".to_string(),
            Self::TypeImplicitNumericCast { .. } => "implicit numeric cast".to_string(),

            Self::DocUnknownArgName { .. } => "unknown @arg parameter".to_string(),
            Self::DocDuplicateArgName { .. } => "duplicate @arg".to_string(),
            Self::DocArgOrReturnsOnNonCallable => "invalid @arg/@returns placement".to_string(),
            Self::DocReturnsOnUnit => "redundant @returns".to_string(),
            Self::DocUnknownDirective { .. } => "unknown documentation directive".to_string(),
            Self::DocUnresolvedRef { .. } => "unresolved documentation @ref".to_string(),
            Self::DocVariantOnNonEnum => "invalid @variant placement".to_string(),
            Self::DocUnknownVariantName { .. } => "unknown @variant name".to_string(),
            Self::DocDuplicateVariantName { .. } => "duplicate @variant".to_string(),
            Self::DocParWithoutGenerics => "invalid @par placement".to_string(),
            Self::DocUnknownGenericName { .. } => "unknown @par type parameter".to_string(),
            Self::DocDuplicateGenericName { .. } => "duplicate @par".to_string(),
            Self::RedundantEnumConstructorParens => "redundant enum constructor parens".to_string(),

            Self::NamingNotPascalCaseType { .. } => "type name not PascalCase".to_string(),
            Self::NamingNotPascalCaseVariant { .. } => "enum variant not PascalCase".to_string(),
            Self::NamingNotCamelCaseField { .. } => "field not lowerCamelCase".to_string(),
            Self::NamingNotPascalCaseCallable { .. } => "callable not PascalCase".to_string(),
            Self::NamingNotPascalCaseModuleSegment { .. } => "module segment not PascalCase".to_string(),
            Self::NamingNotPascalCaseGeneric { .. } => "generic parameter not PascalCase".to_string(),
            Self::NamingNotCamelCaseBinding { .. } => "binding not lowerCamelCase".to_string(),
            Self::NamingNotSnakeCaseTest { .. } => "test name not snake_case".to_string(),
            Self::NamingNotCamelCaseMacro { .. } => "macro name not lowerCamelCase".to_string(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::DuplicateDefinitionName { name, .. } => {
                format!("duplicate definition name `{name}`")
            }
            Self::DuplicateEnumVariant { name, .. } => format!("duplicate enum variant `{name}`"),
            Self::DuplicateContractMethod { name, .. } => {
                format!("duplicate contract method `{name}`")
            }
            Self::DuplicateItemName { name, .. } => format!("duplicate item name `{name}`"),
            Self::UnknownTypeInDefinition { type_name } => {
                format!("unknown type `{type_name}` in definition")
            }
            Self::ConflictingEmbeddedContractMethod {
                contract_name,
                method_name,
            } => format!(
                "embedded contract `{contract_name}` introduces conflicting method `{method_name}`"
            ),
            Self::AmbiguousImport { name, .. } => format!("ambiguous import for `{name}`"),
            Self::UnknownImportPath { path } => format!("unknown import path `{path}`"),
            Self::UseBeforeDeclaration { name } => {
                format!("use of `{name}` before declaration")
            }
            Self::InvalidHirSpan { context } => {
                format!("invalid span invariant in `{context}`")
            }
            Self::UnresolvedHirValuePath => {
                "unresolved value path in HIR legality validation".to_string()
            }
            Self::UnresolvedHirTypePath => {
                "unresolved type path in HIR legality validation".to_string()
            }
            Self::NonNormalizedHirControlFlow { message } => {
                format!("non-normalized control-flow in HIR: {message}")
            }
            Self::DuplicateAttributeDeclarationTarget { target, .. } => {
                format!("duplicate target `{target}` in attribute declaration target list")
            }
            Self::UnknownAttributeDeclarationTarget { target, .. } => {
                format!("unknown attribute declaration target kind `{target}`")
            }
            Self::AttributeTargetNotAllowed {
                attribute, target, ..
            } => {
                format!("attribute `{attribute}` cannot be applied to `{target}`")
            }
            Self::VisibilityModuleNotFound { module_path, .. } => {
                format!("module `{module_path}` not found")
            }
            Self::FileScopedModuleNotFirstItem { module_path } => {
                format!("file-scoped module `{module_path}` must be the first top-level item")
            }
            Self::DuplicateFileScopedModule { module_path } => {
                format!("duplicate file-scoped module declaration `{module_path}`")
            }
            Self::ModuleDeclarationForbiddenInFileScopedModule => {
                "additional `mod` declarations are not allowed in a file-scoped module file"
                    .to_string()
            }
            Self::VisibilityViolationImportPrivate { name, .. } => {
                format!("visibility violation while importing private item `{name}`")
            }
            Self::ExtendTypePrivateMemberAccess {
                member_name,
                type_name,
                ..
            } => {
                format!("extend type `{type_name}` cannot access private member `{member_name}`")
            }
            Self::UnusedImport { path } => format!("unused import `{path}`"),
            Self::UnusedPrivateItem { name } => format!("unused private item `{name}`"),
            Self::ContractMethodNotFound {
                method_name,
                receiver_name,
            } => {
                format!("method `{method_name}` not found in contract `{receiver_name}`")
            }
            Self::ContractImplementationSignatureMismatch { method_name, .. } => {
                format!("contract implementation signature mismatch for `{method_name}`")
            }
            Self::ContractMethodMissingImplementation {
                contract_name,
                method_name,
                ..
            } => {
                format!("contract method `{contract_name}.{method_name}` is missing implementation")
            }
            Self::ImmutableAssignment { name } => {
                format!("cannot assign to immutable binding `{name}`")
            }
            Self::MatchGuardMustBeBoolean => "match guard must be boolean".to_string(),
            Self::MatchArmTypeMismatch { .. } => "match arm type mismatch".to_string(),
            Self::MatchNonExhaustive { enum_name } => {
                format!("non-exhaustive match on enum `{enum_name}`")
            }
            Self::DuplicatePatternBinding { name } => {
                format!("duplicate pattern binding `{name}`")
            }
            Self::UnknownEnumPath {
                enum_name,
                variant_name,
            } => format!("unknown enum path `{enum_name}::{variant_name}`"),
            Self::PatternArityMismatch { expected, actual } => {
                format!("pattern arity mismatch: expected {expected}, got {actual}")
            }
            Self::EnumConstructorArityMismatch { expected, actual } => {
                format!("enum constructor arity mismatch: expected {expected}, got {actual}")
            }
            Self::UnqualifiedEnumConstructor {
                variant_name,
                enum_name,
            } => format!(
                "unqualified enum constructor `{variant_name}`; use `{enum_name}::{variant_name}`"
            ),
            Self::BreakOutsideLoop => "break used outside loop".to_string(),
            Self::ContinueOutsideLoop => "continue used outside loop".to_string(),
            Self::UnreachableCode => "unreachable code".to_string(),
            Self::ResolveDuplicateItem { name, .. } => format!("duplicate item `{name}`"),
            Self::ResolveDuplicateLocal { name, .. } => format!("duplicate local `{name}`"),
            Self::ResolveUnknownValue { name } => format!("unknown value `{name}`"),
            Self::ResolveUnknownType { name } => format!("unknown type `{name}`"),
            Self::ResolveUnknownModulePath { path } => {
                format!("unknown module path `{path}`")
            }
            Self::ResolveUnknownValueInModule { module_path, name } => {
                format!("unknown value `{name}` in module `{module_path}`")
            }
            Self::ResolveUnknownTypeInModule { module_path, name } => {
                format!("unknown type `{name}` in module `{module_path}`")
            }
            Self::ResolveInvalidConformanceTarget { name } => {
                format!("type conformances must target contracts, but `{name}` is not a contract")
            }
            Self::ResolvePrivateItemInModule { module_path, name } => {
                format!("private item `{name}` cannot be accessed from module `{module_path}`")
            }
            Self::ResolveShadowedLocal { name, .. } => format!("shadowed local `{name}`"),
            Self::TypeUnknownType { name } => format!("unknown type `{name}`"),
            Self::TypeUnknownValueType => "unknown value type".to_string(),
            Self::TypeUnknownStructType => "unknown struct type".to_string(),
            Self::TypeInvalidMemberTarget => {
                "member access target is not a struct-like type".to_string()
            }
            Self::TypeUnknownEnumType => "unknown enum type".to_string(),
            Self::TypeUnknownStructField { name } => {
                format!("unknown struct field `{name}`")
            }
            Self::TypeUnknownEnumVariant { name } => {
                format!("unknown enum variant `{name}`")
            }
            Self::TypeMissingStructField { name } => {
                format!("missing struct field `{name}`")
            }
            Self::TypeMissingTypeAnnotation { name } => {
                format!("missing type annotation for `{name}`")
            }
            Self::TypeMissingTypeArguments => "missing type arguments for generic type".to_string(),
            Self::TypeGenericArgumentMismatch { expected, actual } => {
                format!("generic argument mismatch: expected {expected}, got {actual}")
            }
            Self::TypeMismatch {
                expected_name,
                actual_name,
            } => format!("type mismatch: expected {expected_name}, got {actual_name}"),
            Self::TypeMatchArmMismatch {
                expected_name,
                actual_name,
            } => format!("match arm type mismatch: expected {expected_name}, got {actual_name}"),
            Self::TypeCallArityMismatch { expected, actual } => {
                format!("call arity mismatch: expected {expected}, got {actual}")
            }
            Self::TypeCallArgumentMismatch {
                expected_name,
                actual_name,
            } => format!("call argument mismatch: expected {expected_name}, got {actual_name}"),
            Self::TypeEnumConstructorMismatch { expected, actual } => {
                format!("enum constructor arity mismatch: expected {expected}, got {actual}")
            }
            Self::TypeUnknownCallTarget => "unknown call target".to_string(),
            Self::TypeInvalidBinaryOp => "invalid binary operation".to_string(),
            Self::TypeInvalidUnaryOp => "invalid unary operation".to_string(),
            Self::TypeNonBoolCondition => "non-boolean condition".to_string(),
            Self::TypeUnsupportedExpression => "unsupported expression".to_string(),
            Self::TypeInvalidTryTarget => {
                "try operator requires a Result value with an Ok payload".to_string()
            }
            Self::TypeInvalidEventInvocationScope => {
                "events can only be invoked from methods on their declaring type".to_string()
            }
            Self::TypeInvalidEventCapacity => {
                "event capacity must be a positive integer".to_string()
            }
            Self::TypeInvalidEventSubscriptionTarget => {
                "event subscription target must be an event field".to_string()
            }
            Self::SpawnTargetNotFiberCompatible => {
                "spawn target must be a callable with a type-checkable return type".to_string()
            }
            Self::JoinWouldDeadlock => {
                "a fiber cannot join an ancestor fiber handle (would deadlock)".to_string()
            }
            Self::StackReferenceEscapesSpawn => {
                "spawn closure cannot capture stack references from the spawning fiber".to_string()
            }
            Self::AsyncKeywordReserved => {
                "`async` is reserved and not implemented in this language version".to_string()
            }
            Self::AwaitKeywordReserved => {
                "`await` is reserved and not implemented in this language version".to_string()
            }
            Self::TypeReturnMismatch {
                expected_name,
                actual_name,
            } => format!("return type mismatch: expected {expected_name}, got {actual_name}"),
            Self::TypeNonIterableForTarget => {
                "for-in target does not satisfy iterable contract (missing Next())".to_string()
            }
            Self::TypeIterableNextArityMismatch { expected, actual } => {
                format!("iterable Next() arity mismatch: expected {expected}, got {actual}")
            }
            Self::TypeIterableNextReturnNotOption => {
                "iterable Next() must return Option<T>".to_string()
            }
            Self::TypeIterableOptionSomeArityMismatch { expected, actual } => {
                format!("iterable Option::Some payload mismatch: expected {expected}, got {actual}")
            }
            Self::TypeImplicitNumericCast { from, to } => {
                format!("implicit numeric cast from {from} to {to}")
            }
            Self::MacroUnknown { name } => format!("unknown macro `{name}!`"),
            Self::MacroArgumentArityMismatch {
                name,
                expected,
                actual,
            } => format!(
                "macro `{name}!` expects {expected} argument(s), got {actual}"
            ),
            Self::MacroArgumentKindMismatch {
                name,
                parameter,
                expected_kind,
            } => format!(
                "macro `{name}!` argument `{parameter}` does not match fragment kind `{expected_kind}`"
            ),
            Self::MacroMetavariableOutsideBody { name } => {
                format!("`${name}` is only valid inside a macro definition body")
            }
            Self::MacroExpansionDepthExceeded { max_depth } => format!(
                "macro expansion exceeded max depth ({max_depth})"
            ),
            Self::MacroAmbiguousName { name } => {
                format!("ambiguous macro name `{name}` (multiple definitions in scope)")
            }
            Self::MacroDuplicateParameter { name, parameter } => format!(
                "macro `{name}` has duplicate parameter `{parameter}`"
            ),
            Self::QueryBoundsExceeded {
                max_nodes,
                max_depth,
            } => format!(
                "query traversal exceeded bounds (maxNodes={max_nodes}, maxDepth={max_depth})"
            ),
            Self::QueryNodeSpanUnavailable => {
                "span unavailable for the requested node in current syntax generation".to_string()
            }
            Self::QueryPipelineConflict => {
                "query pipeline contains conflicting operations for one target".to_string()
            }
            Self::QueryPipelineStaleGeneration => {
                "query pipeline references a stale syntax generation".to_string()
            }
            Self::CompositionMissingLaunchHost => {
                "composition requires exactly one `launch` host entry point".to_string()
            }
            Self::CompositionMultipleLaunchHosts => {
                "composition allows only one `launch` host entry point".to_string()
            }
            Self::CompositionDependencyCycle { from_id, to_id } => format!(
                "composition dependency cycle between registrations {from_id} and {to_id}"
            ),
            Self::CompositionUnresolvedInject { requested_type } => {
                format!("no registration provides inject type `{requested_type}`")
            }
            Self::CompositionAmbiguousInject { requested_type } => {
                format!("multiple registrations provide inject type `{requested_type}`")
            }
            Self::CompositionScopedOutsideWith => {
                "`with` scope is required for scoped service resolution".to_string()
            }
            Self::CompositionChildScopeWithoutParent { scope_name } => format!(
                "scope `{scope_name}` has no parent in the host scope tree"
            ),
            Self::CompositionWithArgsMismatch { scope_name } => format!(
                "`with {scope_name}(...)` argument list does not match the scope definition"
            ),
            Self::CompositionLaunchTargetNotHost { target_name } => {
                format!("launch target `{target_name}` is not a host definition")
            }
            Self::CompositionHostInheritanceCycle { host_name } => {
                format!("host inheritance cycle detected at `{host_name}`")
            }
            Self::CompositionDuplicateScopeName { scope_name } => {
                format!("duplicate scope name `{scope_name}` in merged host scope tree")
            }
            Self::CompositionHostInModProject => {
                "host definitions are not allowed in compiler mod projects".to_string()
            }
            Self::CompositionLaunchInLibProject => {
                "`launch` is not allowed in library projects".to_string()
            }
            Self::CompositionInjectOnConstructor => {
                "`inject` fields are not allowed on constructors".to_string()
            }
            Self::CompositionOverrideLifetimeMismatch { binding } => format!(
                "registration lifetime override mismatch for `{binding}`"
            ),
            Self::CompositionInvalidScopeQualifier { qualifier } => format!(
                "invalid scope inject qualifier `{qualifier}`"
            ),
            Self::DocUnknownArgName { name } => {
                format!("`@arg({name})` does not match any parameter of this callable")
            }
            Self::DocDuplicateArgName { name } => {
                format!("duplicate `@arg({name})` in the same documentation block")
            }
            Self::DocArgOrReturnsOnNonCallable => {
                "`@arg` / `@returns` are only valid on leading documentation for a function, method, or contract method signature".to_string()
            }
            Self::DocReturnsOnUnit => "`@returns` is redundant when the callable returns `unit`".to_string(),
            Self::DocUnknownDirective { name } => {
                format!("unknown documentation directive `@{name}`")
            }
            Self::DocUnresolvedRef { path } => {
                format!("documentation `@ref` does not resolve: `{path}`")
            }
            Self::DocVariantOnNonEnum => {
                "`@variant(...)` is only valid on leading documentation for an `enum` declaration"
                    .to_string()
            }
            Self::DocUnknownVariantName { name } => {
                format!("`@variant({name})` does not match any variant of this enum")
            }
            Self::DocDuplicateVariantName { name } => {
                format!("duplicate `@variant({name})` in the same documentation block")
            }
            Self::DocParWithoutGenerics => {
                "`@par(...)` requires this declaration to declare generic type parameters".to_string()
            }
            Self::DocUnknownGenericName { name } => {
                format!("`@par({name})` does not match any generic type parameter on this item")
            }
            Self::DocDuplicateGenericName { name } => {
                format!("duplicate `@par({name})` in the same documentation block")
            }
            Self::RedundantEnumConstructorParens => {
                "redundant empty parentheses on nullary enum constructor".to_string()
            }
            Self::NamingNotPascalCaseType { name } => {
                format!("type name `{name}` should use PascalCase")
            }
            Self::NamingNotPascalCaseVariant { name } => {
                format!("enum variant `{name}` should use PascalCase")
            }
            Self::NamingNotCamelCaseField { name } => {
                format!("field `{name}` should use lowerCamelCase")
            }
            Self::NamingNotPascalCaseCallable { name } => {
                format!("callable `{name}` should use PascalCase")
            }
            Self::NamingNotPascalCaseModuleSegment { segment } => {
                format!("module segment `{segment}` should use PascalCase")
            }
            Self::NamingNotPascalCaseGeneric { name } => {
                format!("generic type parameter `{name}` should use PascalCase")
            }
            Self::NamingNotCamelCaseBinding { name } => {
                format!("binding `{name}` should use lowerCamelCase")
            }
            Self::NamingNotSnakeCaseTest { name } => {
                format!("test name `{name}` should use snake_case")
            }
            Self::NamingNotCamelCaseMacro { name } => {
                format!("macro `{name}` should use lowerCamelCase")
            }
        }
    }

    pub fn help(&self) -> Option<String> {
        match self {
            Self::DuplicateDefinitionName { previous, .. }
            | Self::DuplicateEnumVariant { previous, .. }
            | Self::DuplicateContractMethod { previous, .. }
            | Self::DuplicateItemName { previous, .. }
            | Self::ResolveDuplicateItem { previous, .. }
            | Self::ResolveDuplicateLocal { previous, .. }
            | Self::ResolveShadowedLocal { previous, .. }
            | Self::AmbiguousImport { previous, .. } => Some(format!(
                "previously defined at line {}, column {}",
                previous.line_col_start.0, previous.line_col_start.1
            )),
            Self::DuplicateAttributeDeclarationTarget { previous, .. } => Some(format!(
                "target already listed at line {}, column {}",
                previous.line_col_start.0, previous.line_col_start.1
            )),
            Self::UnknownAttributeDeclarationTarget { allowed, .. }
            | Self::AttributeTargetNotAllowed { allowed, .. } => {
                Some(format!("allowed targets: {}", allowed.join(", ")))
            }
            Self::VisibilityModuleNotFound { file_candidate, mod_candidate, .. } => {
                Some(format!("expected `{file_candidate}` or `{mod_candidate}`"))
            }
            Self::FileScopedModuleNotFirstItem { .. } => Some("move `mod ...;` to the top of the file".to_string()),
            Self::DuplicateFileScopedModule { .. } | Self::ModuleDeclarationForbiddenInFileScopedModule => {
                Some("keep a single top-level `mod ...;` and remove other module declarations".to_string())
            }
            Self::VisibilityViolationImportPrivate { private_span, .. } => Some(format!(
                "item is private (declared at line {}, column {})",
                private_span.line_col_start.0, private_span.line_col_start.1
            )),
            Self::ExtendTypePrivateMemberAccess { private_span, .. } => Some(format!(
                "member is private (declared at line {}, column {})",
                private_span.line_col_start.0, private_span.line_col_start.1
            )),
            Self::ContractImplementationSignatureMismatch { expected, actual, .. } => {
                Some(format!("expected `{expected}`, got `{actual}`"))
            }
            Self::ContractMethodMissingImplementation { expected, .. } => {
                Some(format!("expected signature `{expected}`"))
            }
            Self::ImmutableAssignment { .. } => Some("declare it as `let mut` to allow assignment".to_string()),
            Self::MatchArmTypeMismatch { expected, actual } => Some(format!("expected `{expected}`, got `{actual}`")),
            Self::ResolvePrivateItemInModule { .. } => {
                Some("mark the item `pub` or avoid cross-module access".to_string())
            }
            Self::ResolveInvalidConformanceTarget { .. } => {
                Some("declare conformance against a `contract`, not a type/enum/function".to_string())
            }
            Self::TypeMissingTypeArguments => Some("provide explicit type arguments, e.g. `Type<i32>`".to_string()),
            Self::TypeInvalidTryTarget => Some("apply `?` only to a `Result<Ok, Error>` expression".to_string()),
            Self::TypeInvalidEventSubscriptionTarget => Some(
                "use `+=`/`-=` with an event field declared as `event Name(...)` or `event{N} Name(...)`".to_string(),
            ),
            Self::TypeImplicitNumericCast { .. } => {
                Some("add an explicit cast to make conversion intent clear".to_string())
            }
            Self::UnreachableCode => {
                Some("remove this statement or move it before the terminating statement".to_string())
            }
            Self::DocUnresolvedRef { .. } => Some("use a name that exists in this compilation unit".to_string()),
            Self::RedundantEnumConstructorParens => Some("remove the redundant empty parentheses".to_string()),
            Self::NamingNotPascalCaseType { name }
            | Self::NamingNotPascalCaseVariant { name }
            | Self::NamingNotPascalCaseCallable { name }
            | Self::NamingNotPascalCaseModuleSegment { segment: name }
            | Self::NamingNotPascalCaseGeneric { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::PascalCase)
            )),
            Self::NamingNotCamelCaseField { name }
            | Self::NamingNotCamelCaseBinding { name }
            | Self::NamingNotCamelCaseMacro { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::LowerCamelCase,)
            )),
            Self::NamingNotSnakeCaseTest { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::SnakeCase)
            )),
            _ => None,
        }
    }
}
