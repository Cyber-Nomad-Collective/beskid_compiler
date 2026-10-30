use super::SemanticIssueKind;

impl SemanticIssueKind {
    pub fn message(&self) -> String {
        let text = self.message_text();
        if self.names_module_path() { text.replace("::", ".") } else { text }
    }

    /// Whether this kind's text names module paths, which Beskid writes with `.` (`Core.Text`);
    /// a resolver that still joins segments with `::` is rendered the same way.
    pub(super) fn names_module_path(&self) -> bool {
        matches!(
            self,
            Self::UnknownImportPath { .. }
                | Self::UnusedImport { .. }
                | Self::VisibilityModuleNotFound { .. }
                | Self::FileScopedModuleNotFirstItem { .. }
                | Self::DuplicateFileScopedModule { .. }
                | Self::ResolveUnknownModulePath { .. }
                | Self::ResolveUnknownValueInModule { .. }
                | Self::ResolveUnknownTypeInModule { .. }
                | Self::ResolvePrivateItemInModule { .. }
                | Self::ResolveAmbiguousModulePath { .. }
                | Self::ResolveAmbiguousItemInModule { .. }
                | Self::ResolveAmbiguousModuleQualifier { .. }
                | Self::MissingImport { .. }
                | Self::MissingImportAmbiguous { .. }
                | Self::EnumValueMethodCall { .. }
        )
    }

    fn message_text(&self) -> String {
        match self {
            Self::DuplicateDefinitionName { name, .. } => {
                format!("duplicate definition name `{name}`")
            }
            Self::DuplicateEnumVariant { name, .. } => format!("duplicate enum variant `{name}`"),
            Self::DuplicateContractMethod { name, .. } => {
                format!("duplicate contract method `{name}`")
            }
            Self::UnknownTypeInDefinition { type_name } => {
                format!("unknown type `{type_name}` in definition")
            }
            Self::ConflictingEmbeddedContractMethod {
                contract_name,
                method_name,
            } => format!(
                "embedded contract `{contract_name}` introduces conflicting method `{method_name}`"
            ),
            Self::UnknownImportPath { path, .. } => format!("unknown import path `{path}`"),
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
            Self::AttributeTargetNotAllowed { attribute, target, declared: true, .. } => {
                format!("attribute `{attribute}` cannot be applied to `{target}`")
            }
            Self::AttributeTargetNotAllowed { attribute, target, declared: false, .. } => {
                format!("attribute `{attribute}` on `{target}` has no declaration in scope")
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
            Self::UnusedPrivateItem { name, .. } => format!("`{name}` is never used"),
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
            Self::ContractAssociatedTypeMissingBinding { contract_name, assoc_name } => {
                format!("`{contract_name}::{assoc_name}` has no default and no implementor binding")
            }
            Self::ThisUsedOutsideContractOrImpl => "`This` used outside a contract or impl-block scope".to_string(),
            Self::UnresolvedAssociatedType { name } => format!("unresolved associated type `{name}`"),
            Self::GenericBoundNotSatisfied { type_name, contract_name, .. } => format!(
                "generic bound not satisfied: `{}` does not conform to `{}`",
                super::display_type(type_name),
                super::display_type(contract_name)
            ),
            Self::InvalidPrimitiveImplementation { type_name, .. } => {
                format!("invalid implementation for primitive type `{type_name}`")
            }
            Self::ImmutableAssignment { name, .. } => {
                format!("`{name}` is assigned again but is not mutable")
            }
            Self::MatchGuardMustBeBoolean => "match guard must be boolean".to_string(),
            Self::MatchArmTypeMismatch { .. } => "match arm type mismatch".to_string(),
            Self::MatchNonExhaustive { enum_name, missing_pattern } => {
                format!("non-exhaustive match on enum `{enum_name}`: `{missing_pattern}` not covered")
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
            Self::ResolveUnknownValue { name, .. } => format!("unknown value `{name}`"),
            Self::ResolveUnknownType { name } => format!("unknown type `{name}`"),
            Self::ResolveUnknownModulePath { path } => format!("unknown module path `{path}`"),
            Self::ResolveUnknownValueInModule { module_path, name } => {
                format!("unknown value `{name}` in module `{module_path}`")
            }
            Self::ResolveUnknownTypeInModule { module_path, name } => {
                format!("unknown type `{name}` in module `{module_path}`")
            }
            Self::ResolveInvalidConformanceTarget { name } => {
                format!("type conformances must target contracts, but `{name}` is not a contract")
            }
            Self::ResolvePrivateRuntimeBuiltin { name } => {
                format!("`{name}` is a private runtime builtin and cannot be called from this source unit")
            }
            Self::ResolvePrivateItemInModule { module_path, name } => {
                format!("private item `{name}` cannot be accessed from module `{module_path}`")
            }
            Self::ResolveAmbiguousModulePath { path, candidates } => {
                format!("ambiguous module path `{path}`: {candidates} assembled modules declare it")
            }
            Self::ResolveAmbiguousItemInModule { module_path, name, candidates } => format!(
                "ambiguous item `{name}` in module `{module_path}`: {candidates} re-exported items share the name"
            ),
            Self::ResolveAmbiguousModuleQualifier { name, member, import_path } => format!(
                "ambiguous module `{name}`: both the inline module `{name}` and the import `{import_path}` declare `{member}`"
            ),
            Self::ResolveShadowedLocal { name, .. } => format!("shadowed local `{name}`"),
            Self::MissingImport { name, module_path } => {
                format!("unresolved name `{name}` — did you mean to import `{module_path}.{name}`?")
            }
            Self::MissingImportAmbiguous { name, candidates } => {
                format!("unresolved name `{name}` — candidates: {}", candidates.join(", "))
            }
            Self::TypeUnknownType { name } => format!("unknown type `{name}`"),
            Self::TypeUnknownValueType => "unknown value type".to_string(),
            Self::TypeUnknownStructType => "unknown type in a type literal".to_string(),
            Self::TypeInvalidMemberTarget => {
                "member access target is not a type with fields".to_string()
            }
            Self::TypeUnknownEnumType => "unknown enum type".to_string(),
            Self::TypeUnknownStructField { name, type_name: Some(type_name), .. } => {
                format!("type `{}` has no field `{name}`", super::display_type(type_name))
            }
            Self::TypeUnknownStructField { name, type_name: None, .. } => format!("unknown field `{name}`"),
            Self::TypeInaccessibleStructField { name } => {
                format!("field `{name}` is private")
            }
            Self::TypeUnknownEnumVariant { name } => {
                format!("unknown enum variant `{name}`")
            }
            Self::TypeMissingStructField { name } => {
                format!("missing field `{name}` in the type literal")
            }
            Self::TypeMissingTypeAnnotation { name } => {
                format!("missing type annotation for `{name}`")
            }
            Self::TypeMissingTypeArguments { parameter: None } => "missing type arguments for generic type".to_string(),
            Self::TypeMissingTypeArguments { parameter: Some(parameter) } => {
                format!("cannot infer type parameter `{parameter}` of this call")
            }
            Self::TypeGenericArgumentMismatch { expected, actual } => {
                format!("generic argument mismatch: expected {expected}, got {actual}")
            }
            Self::TypeGenericParameterConflict {
                parameter,
                first_name,
                second_name,
            } => format!(
                "generic parameter `{parameter}` cannot be both `{first_name}` and `{second_name}`"
            ),
            Self::TypeMismatch {
                expected_name,
                actual_name,
            } => {
                let (expected, actual) = super::display_type_pair(expected_name, actual_name);
                format!("type mismatch: expected `{expected}`, got `{actual}`")
            }
            Self::TypeMatchArmMismatch {
                expected_name,
                actual_name,
            } => {
                let (expected, actual) = super::display_type_pair(expected_name, actual_name);
                format!("match arm type mismatch: expected `{expected}`, got `{actual}`")
            }
            Self::TypeCallArityMismatch { required, total, actual } => {
                let expected = if required == total {
                    format!("{total} argument{}", if *total == 1 { "" } else { "s" })
                } else {
                    format!("{required}\u{2013}{total} arguments")
                };
                // Within the range the call only omitted optional arguments, which the compiler fills
                // once the callee is known; reaching here means it could not fill them.
                if (*required..*total).contains(actual) {
                    format!(
                        "call arity mismatch: expected {expected}, got {actual}; the omitted optional arguments \
                         could not be filled for this call, pass them explicitly"
                    )
                } else {
                    format!("call arity mismatch: expected {expected}, got {actual}")
                }
            }
            Self::RequiredParameterAfterOptional { name } => {
                format!("required parameter `{name}` follows an optional parameter")
            }
            Self::DefaultValueNotConstant { name } => {
                format!("default value of `{name}` is not a compile-time constant")
            }
            Self::DefaultValueTypeMismatch { name, value, expected } => {
                format!("default value `{value}` does not fit parameter `{name}` of type `{expected}`")
            }
            Self::OptionalParameterNotAllowed { place } if place.contains("host or scope") => {
                format!("caller information is not allowed on {place}")
            }
            Self::OptionalParameterNotAllowed { place } => format!("{place} cannot be optional"),
            Self::CallerInformationWithoutDefault { name, attribute, .. } => {
                format!("`[{attribute}]` parameter `{name}` needs a default value")
            }
            Self::ContractDefaultValueMismatch { method_name, parameter, expected, actual } => format!(
                "`{method_name}` gives parameter `{parameter}` the default `{actual}`, but the contract declares {expected}"
            ),
            Self::CallerInformationParameterType { name, attribute, expected } => {
                format!("`[{attribute}]` parameter `{name}` must have type `{expected}`")
            }
            Self::TypeCallArgumentMismatch {
                expected_name,
                actual_name,
            } => {
                let (expected, actual) = super::display_type_pair(expected_name, actual_name);
                format!("call argument mismatch: expected `{expected}`, got `{actual}`")
            }
            Self::TypeEnumConstructorMismatch { expected, actual } => {
                format!("enum constructor arity mismatch: expected {expected}, got {actual}")
            }
            Self::TypeUnknownCallTarget => "unknown call target".to_string(),
            Self::TypeInvalidBinaryOp => "invalid binary operation".to_string(),
            Self::TypeStringConcatOperandNotConvertible { found } => {
                format!("a value of type `{found}` cannot be concatenated or interpolated into a string")
            }
            Self::TypeInvalidCompoundAssignment { operator, type_name } => {
                format!("`{operator}` is not defined for `{}`", super::display_type(type_name))
            }
            Self::TypeInvalidUnaryOp => "invalid unary operation".to_string(),
            Self::TypeNonBoolCondition => "non-boolean condition".to_string(),
            Self::TypeUnsupportedExpression => "unsupported expression".to_string(),
            Self::TypeInvalidTryTarget => {
                "try operator requires a Result value with an Ok payload".to_string()
            }
            Self::TypeInvalidPrimitiveConversionArgument => {
                "primitive numeric conversion requires exactly one primitive numeric argument".to_string()
            }
            Self::ScopedCleanupRejected { reason } => format!("scoped use rejected: {reason}"),
            Self::InternalSemanticFactUnavailable { .. }
            | Self::InternalLoweringRuleMissing { .. }
            | Self::InternalLoweringInvariant { .. }
            | Self::InternalModSemanticFailure { .. }
            | Self::InternalBuildFailure { .. } => self.internal_message().unwrap_or_default(),
            Self::CopyOnAssignWithoutCopy { type_name, problem } => {
                format!("`[CopyOnAssign]` type `{type_name}` has no valid `Copy` method: {problem}")
            }
            Self::DiscardedArrayGrowth { operation, owner, .. } => {
                format!("the result of `Array.{operation}` is discarded; the call works on a copy, so `{owner}` is not changed")
            }
            Self::DeadParameterAssignment { parameter } => {
                format!("the value assigned to the `mut` parameter `{parameter}` is never read")
            }
            Self::DeadCollectionGrowth => "the grown handle of a `mut T[]` parameter is discarded and the body never \
                                           publishes the parameter, so the caller keeps the ungrown array"
                .to_string(),
            Self::LambdaCapturesStackReference { name } => {
                format!("lambda cannot capture `{name}`: it is a mutable binding or a native pointer")
            }
            Self::UseAfterMove { name } => format!("`{name}` is used after it was moved"),
            Self::CapturedLocalAssignedInBoundLambda { name } => {
                format!("a lambda passed to a `[Captures]` parameter cannot assign the captured local `{name}`")
            }
            Self::CaptureNotTransferable { name, type_name, class } => {
                let type_name = super::display_type(type_name);
                let reason = match class.as_str() {
                    "Never" => format!("a `{type_name}` value never leaves the task that created it"),
                    "Moved" => format!("this lambda's bound does not accept `{type_name}` values that are handed over"),
                    "Copied" => format!("this lambda's bound does not accept `{type_name}` values that are copied across"),
                    _ => format!("this lambda's bound does not accept `{type_name}` values"),
                };
                format!("`{name}` cannot be captured here because {reason}")
            }
            Self::MovedCaptureWithoutMove { name } => {
                format!("`{name}` is a Moved value and must be captured with `move`")
            }
            Self::CapturesArgumentNotLiteral => {
                "the argument to a `[Captures]` parameter must be a lambda literal or a function item".to_string()
            }
            Self::MoveOfNonOwningName { name, class } => match class {
                Some(class) => format!("`move` needs a Moved-class owning name, but `{name}` has class {class}"),
                None => format!("`move {name}` is not a crossing operand: `move` applies only when a value crosses to another isolate"),
            },
            Self::MoveInLoopWithoutReassignment { name } => format!(
                "`{name}` is moved inside a loop body and not reassigned before the next iteration"
            ),
            Self::BorrowEscapes { name } => {
                format!("`{name}` is not the only name for its Moved value, so it cannot cross to another isolate")
            }
            Self::NestedCopyOnAssignCrossing { name, type_name } => format!(
                "`{name}` holds a nested `{type_name}` that may share its storage with a list this isolate keeps, so it cannot cross to another isolate"
            ),
            Self::BorrowedParameterKept { name, parameter, .. } if name == parameter => format!(
                "the parameter `{name}` is borrowed, so it cannot be stored, returned, sent to another isolate or kept: the caller still owns its value"
            ),
            Self::BorrowedParameterKept { name, parameter, .. } => format!(
                "`{name}` holds the borrowed parameter `{parameter}`, so it cannot be stored, returned, sent to another isolate or kept: the caller still owns its value"
            ),
            Self::ScopeBorrowedEscapes { name, type_name } => format!(
                "`{name}` of the scope-borrowed type `{type_name}` cannot be stored, returned, passed to `spawn` or captured by an escaping closure"
            ),
            Self::ImplicitAliasOfMovedValue { name } => {
                format!("`{name}` crosses to another isolate without `move`")
            }
            Self::DestructiveReadWithoutEmpty { field, type_name } => format!(
                "`move` cannot read field `{field}`: its type `{}` has no canonical empty value",
                super::display_type(type_name)
            ),
            Self::ImmutableFieldNotShared { type_name, field, class } => format!(
                "field `{field}` of `[Immutable]` type `{}` must be Shared, but it is {class}",
                super::display_type(type_name)
            ),
            Self::ImmutableFieldAssignment { type_name, field } => format!(
                "field `{field}` of `[Immutable]` type `{}` cannot be assigned or escape outside construction",
                super::display_type(type_name)
            ),
            Self::MarkerOnNeverTransferable { marker, type_name, cause } => {
                format!("`[{marker}]` cannot mark `{}`: {cause} is never transferable", super::display_type(type_name))
            }
            Self::MarkerFieldClassMismatch { marker, type_name, field, class } => format!(
                "field `{field}` of `[{marker}]` type `{}` is {class}, which `[{marker}]` does not admit",
                super::display_type(type_name)
            ),
            Self::CapturesBoundNotMarker { bound } => match bound {
                Some(bound) => format!("`[Captures]` bound `{bound}` is not a marker contract"),
                None => "`[Captures]` applies only to a parameter of function type".to_string(),
            },
            Self::InjectInsideIsolateBody { .. } => "composition is not available inside an isolate".to_string(),
            Self::GenericReceiverMethodUndeclared { method, parameter } => {
                format!("no `where` bound of type parameter `{parameter}` declares a method `{method}`")
            }
            Self::EnumValueMethodCall { method, enum_name } => {
                format!("`{enum_name}` is an enum and has no method `{method}`")
            }
            Self::CompilerDerivedConformance { contract } => format!(
                "`{contract}` is compiler-derived and cannot be listed as a conformance; the compiler proves it from the \
                 type's fields and markers"
            ),
            Self::CompilerDerivedNotMarker { contract } => format!(
                "`[CompilerDerived]` applies only to the marker contracts `Transferable`, `Shared`, `Moved` and \
                 `Copied`, not to `{contract}`"
            ),
            Self::DerivedContractMember { contract, member } => {
                format!("compiler-derived contract `{contract}` cannot declare the member `{member}`")
            }
            Self::ExclusiveMarkerAttribute { marker, previous } => {
                format!("`[{marker}]` cannot be combined with `[{previous}]`: the markers are exclusive")
            }
            Self::CorelibOnlyAttribute { attribute } => {
                format!("`[{attribute}]` is available only inside corelib (`Core.*` modules)")
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
            } => {
                let (expected, actual) = super::display_type_pair(expected_name, actual_name);
                format!("return type mismatch: expected `{expected}`, got `{actual}`")
            }
            Self::TypeMissingReturn { callable, .. } => {
                format!("`{callable}` can reach the end of its body without returning a value")
            }
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
            Self::NamingNotPascalCaseTest { name } => {
                format!("test name `{name}` should use PascalCase")
            }
            Self::NamingNotCamelCaseMacro { name } => {
                format!("macro `{name}` should use lowerCamelCase")
            }
            Self::NumericLiteralOutOfRange { value, primitive } => {
                format!("`{value}` does not fit in `{primitive}`")
            }
            Self::ExternInvalidAbi { .. } => "invalid extern ABI; only Abi:\"C\" is supported".to_string(),
            Self::ExternMissingLibrary => "extern contract missing Library".to_string(),
            Self::ExternDisallowedParamType { method, .. } => {
                format!("extern param type not allowed in `{method}`")
            }
            Self::ExternDisallowedReturnType { method, .. } => {
                format!("extern return type not allowed in `{method}`")
            }
            Self::GlueBindingRejected { method, .. } => {
                format!("Glue binding rejected for extern method `{method}`")
            }
            Self::ExternInvalidOptional { .. } => "invalid optional extern contract".to_string(),
            Self::InvalidClifBlock { .. } => "invalid clif block".to_string(),
        }
    }
}
