use super::SemanticIssueKind;

impl SemanticIssueKind {
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
            Self::InvalidSyntaxSpan { context } => {
                format!("invalid span invariant in `{context}`")
            }
            Self::UnresolvedSyntaxValuePath => {
                "unresolved value path in syntax legality validation".to_string()
            }
            Self::UnresolvedSyntaxTypePath => {
                "unresolved type path in syntax legality validation".to_string()
            }
            Self::NonCanonicalSyntaxControlFlow { message } => {
                format!("non-normalized control-flow in syntax: {message}")
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
            Self::MissingImport { name, module_path } => {
                format!("unresolved name `{name}` — did you mean to import `{module_path}::{name}`?")
            }
            Self::MissingImportAmbiguous { name, candidates } => {
                format!("unresolved name `{name}` — candidates: {}", candidates.join(", "))
            }
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
}
