use super::SemanticIssueKind;

impl SemanticIssueKind {
    pub fn help(&self) -> Option<String> {
        match self {
            // ── Definition / duplicate-name diagnostics ──
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
            Self::UnknownTypeInDefinition { type_name } => Some(format!(
                "check the spelling of `{type_name}` or import the type if it's defined in another module"
            )),
            Self::ConflictingEmbeddedContractMethod { contract_name, method_name } => Some(format!(
                "remove the duplicate `{method_name}` from the embedded contract `{contract_name}` or rename it"
            )),

            // ── Import / syntax diagnostics ──
            Self::UnknownImportPath { path } => Some(format!(
                "check the module path `{path}` — it may not exist or may not be exported"
            )),
            Self::UseBeforeDeclaration { name } => Some(format!(
                "move the `use {name}` declaration before its first reference"
            )),
            Self::InvalidSyntaxSpan { context } => Some(format!(
                "this is an internal compiler error in `{context}` — please report it"
            )),
            Self::UnresolvedSyntaxValuePath => Some(
                "the value path could not be resolved — check spelling and imports".to_string()
            ),
            Self::UnresolvedSyntaxTypePath => Some(
                "the type path could not be resolved — check spelling and imports".to_string()
            ),
            Self::NonCanonicalSyntaxControlFlow { message } => {
                Some(format!("normalize the control flow: {message}"))
            }
            Self::DuplicateAttributeDeclarationTarget { previous, .. } => Some(format!(
                "target already listed at line {}, column {}",
                previous.line_col_start.0, previous.line_col_start.1
            )),
            Self::UnknownAttributeDeclarationTarget { allowed, .. }
            | Self::AttributeTargetNotAllowed { allowed, .. } => {
                Some(format!("allowed targets: {}", allowed.join(", ")))
            }

            // ── Visibility / module diagnostics ──
            Self::VisibilityModuleNotFound { file_candidate, mod_candidate, .. } => {
                Some(format!("expected `{file_candidate}` or `{mod_candidate}`"))
            }
            Self::FileScopedModuleNotFirstItem { .. } => {
                Some("move `mod ...;` to the top of the file".to_string())
            }
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
            Self::UnusedImport { path } => {
                Some(format!("remove the unused `use {path}` or use the imported item"))
            }
            Self::UnusedPrivateItem { name } => Some(format!(
                "remove the private item `{name}` or mark it `pub` if it's needed externally"
            )),

            // ── Contract conformance diagnostics ──
            Self::ContractMethodNotFound { method_name, receiver_name } => Some(format!(
                "check that the contract declares a method `{method_name}` and that `{receiver_name}` conforms to it"
            )),
            Self::ContractImplementationSignatureMismatch { expected, actual, .. } => {
                Some(format!("expected `{expected}`, got `{actual}`"))
            }
            Self::ContractMethodMissingImplementation { expected, .. } => {
                Some(format!("expected signature `{expected}`"))
            }
            Self::ImmutableAssignment { .. } => {
                Some("declare it as `let mut` to allow assignment".to_string())
            }

            // ── Pattern / match / control-flow diagnostics ──
            Self::MatchGuardMustBeBoolean => {
                Some("the match guard expression must evaluate to `bool`".to_string())
            }
            Self::MatchArmTypeMismatch { expected, actual } => {
                Some(format!("expected `{expected}`, got `{actual}`"))
            }
            Self::MatchNonExhaustive { enum_name } => Some(format!(
                "add a match arm for every variant of `{enum_name}` or a wildcard `_` arm"
            )),
            Self::DuplicatePatternBinding { name } => Some(format!(
                "rename one of the bindings named `{name}` in this pattern"
            )),
            Self::UnknownEnumPath { enum_name, variant_name } => Some(format!(
                "check that enum `{enum_name}` has variant `{variant_name}` and is imported"
            )),
            Self::PatternArityMismatch { expected, actual } => Some(format!(
                "the pattern expects {expected} bindings but got {actual}"
            )),
            Self::EnumConstructorArityMismatch { expected, actual } => Some(format!(
                "the constructor expects {expected} arguments but got {actual}"
            )),
            Self::UnqualifiedEnumConstructor { variant_name, enum_name } => Some(format!(
                "qualify as `{enum_name}::{variant_name}` or add `use {enum_name}::{variant_name}`"
            )),
            Self::BreakOutsideLoop => {
                Some("remove this `break` or move it inside a loop body".to_string())
            }
            Self::ContinueOutsideLoop => {
                Some("remove this `continue` or move it inside a loop body".to_string())
            }
            Self::UnreachableCode => {
                Some("remove this statement or move it before the terminating statement".to_string())
            }

            // ── Resolution diagnostics ──
            Self::ResolveUnknownValue { name } => Some(format!(
                "check the spelling of `{name}` or add a `use` import if it's defined elsewhere"
            )),
            Self::ResolveUnknownType { name } => Some(format!(
                "check the spelling of `{name}` or add a `use` import if it's defined elsewhere"
            )),
            Self::ResolveUnknownModulePath { path } => Some(format!(
                "check the module path `{path}` — it may not exist or may not be in scope"
            )),
            Self::ResolveUnknownValueInModule { module_path, name } => Some(format!(
                "check that module `{module_path}` exports a value named `{name}`"
            )),
            Self::ResolveUnknownTypeInModule { module_path, name } => Some(format!(
                "check that module `{module_path}` exports a type named `{name}`"
            )),
            Self::ResolveInvalidConformanceTarget { .. } => Some(
                "declare conformance against a `contract`, not a type/enum/function".to_string()
            ),
            Self::ResolvePrivateItemInModule { .. } => {
                Some("mark the item `pub` or avoid cross-module access".to_string())
            }
            Self::MissingImport { name, module_path } => {
                Some(format!("add `use {module_path}::{name};` at the top of the file"))
            }
            Self::MissingImportAmbiguous { name, candidates } => Some(format!(
                "add one of: {}",
                candidates
                    .iter()
                    .map(|c| format!("`use {c}::{name};`"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )),

            // ── Type-checking diagnostics ──
            Self::TypeUnknownType { name } => Some(format!(
                "check the spelling of `{name}` or import the type if it's defined in another module"
            )),
            Self::TypeUnknownValueType => Some(
                "the value's type could not be determined — add a type annotation".to_string()
            ),
            Self::TypeUnknownStructType => Some(
                "the struct type could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeInvalidMemberTarget => Some(
                "member access requires a struct-like type — check the receiver type".to_string()
            ),
            Self::TypeUnknownEnumType => Some(
                "the enum type could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeUnknownStructField { name } => {
                Some(format!("check that the struct has a field named `{name}`"))
            }
            Self::TypeUnknownEnumVariant { name } => {
                Some(format!("check that the enum has a variant named `{name}`"))
            }
            Self::TypeMissingStructField { name } => {
                Some(format!("add the missing field `{name}` to the struct literal"))
            }
            Self::TypeMissingTypeAnnotation { name } => Some(format!(
                "add an explicit type annotation for `{name}`, e.g. `let {name}: i32 = ...`"
            )),
            Self::TypeMissingTypeArguments => {
                Some("provide explicit type arguments, e.g. `Type<i32>`".to_string())
            }
            Self::TypeGenericArgumentMismatch { expected, actual } => Some(format!(
                "the generic type expects {expected} argument(s) but got {actual}"
            )),
            Self::TypeMismatch { expected_name, actual_name } => Some(format!(
                "expected `{expected_name}` but got `{actual_name}` — check the expression or add a cast"
            )),
            Self::TypeMatchArmMismatch { expected_name, actual_name } => Some(format!(
                "expected `{expected_name}` but got `{actual_name}` — unify the arm types or add a cast"
            )),
            Self::TypeCallArityMismatch { expected, actual } => Some(format!(
                "the callable expects {expected} argument(s) but got {actual}"
            )),
            Self::TypeCallArgumentMismatch { expected_name, actual_name } => Some(format!(
                "expected argument type `{expected_name}` but got `{actual_name}`"
            )),
            Self::TypeEnumConstructorMismatch { expected, actual } => Some(format!(
                "the enum constructor expects {expected} argument(s) but got {actual}"
            )),
            Self::TypeUnknownCallTarget => Some(
                "the call target could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeInvalidBinaryOp => Some(
                "the binary operator is not valid for these operand types".to_string()
            ),
            Self::TypeInvalidUnaryOp => Some(
                "the unary operator is not valid for this operand type".to_string()
            ),
            Self::TypeNonBoolCondition => Some("the condition must evaluate to `bool`".to_string()),
            Self::TypeUnsupportedExpression => Some(
                "this expression form is not supported in the current context".to_string()
            ),
            Self::TypeInvalidTryTarget => {
                Some("apply `?` only to a `Result<Ok, Error>` expression".to_string())
            }
            Self::TypeInvalidEventInvocationScope => Some(
                "events can only be raised from within methods on their declaring type".to_string()
            ),
            Self::TypeInvalidEventCapacity => {
                Some("event capacity must be a positive integer literal".to_string())
            }
            Self::TypeInvalidEventSubscriptionTarget => Some(
                "use `+=`/`-=` with an event field declared as `event Name(...)` or `event{N} Name(...)`".to_string()
            ),
            Self::SpawnTargetNotFiberCompatible => Some(
                "the spawn target must be a callable with a type-checkable return type".to_string()
            ),
            Self::JoinWouldDeadlock => Some(
                "a fiber cannot join an ancestor fiber — this would deadlock".to_string()
            ),
            Self::StackReferenceEscapesSpawn => Some(
                "move captured values into the spawn closure or use heap allocation".to_string()
            ),
            Self::AsyncKeywordReserved => Some(
                "`async` is not implemented — use fibers and `spawn` instead".to_string()
            ),
            Self::AwaitKeywordReserved => Some(
                "`await` is not implemented — use fiber `join` instead".to_string()
            ),
            Self::TypeReturnMismatch { expected_name, actual_name } => Some(format!(
                "the return expression produces `{actual_name}` but the function expects `{expected_name}`"
            )),
            Self::TypeNonIterableForTarget => Some(
                "the for-in target must implement the iterable contract (provide a `Next()` method returning `Option<T>`)".to_string()
            ),
            Self::TypeIterableNextArityMismatch { expected, actual } => Some(format!(
                "`Next()` must take {expected} parameter(s) but takes {actual}"
            )),
            Self::TypeIterableNextReturnNotOption => Some(
                "`Next()` must return `Option<T>` — wrap the result in `Some(...)` or `None`".to_string()
            ),
            Self::TypeIterableOptionSomeArityMismatch { expected, actual } => Some(format!(
                "`Option::Some` expects {expected} payload(s) but got {actual}"
            )),
            Self::TypeImplicitNumericCast { .. } => {
                Some("add an explicit cast to make conversion intent clear".to_string())
            }

            // ── Macro diagnostics ──
            Self::MacroUnknown { name } => {
                Some(format!("check the spelling of `{name}` or import the macro definition"))
            }
            Self::MacroArgumentArityMismatch { name, expected, actual } => Some(format!(
                "`{name}!` expects {expected} argument(s) — remove {actual} extra or add the missing ones"
            )),
            Self::MacroArgumentKindMismatch { name, parameter, expected_kind } => Some(format!(
                "`{name}!` parameter `{parameter}` must be a `{expected_kind}` fragment"
            )),
            Self::MacroMetavariableOutsideBody { name } => Some(format!(
                "`${name}` is only valid inside a macro body — move it inside"
            )),
            Self::MacroExpansionDepthExceeded { max_depth } => Some(format!(
                "macro expansion exceeded {max_depth} levels — simplify the macro or increase the limit"
            )),
            Self::MacroAmbiguousName { name } => Some(format!(
                "multiple macros named `{name}` are in scope — qualify the import"
            )),
            Self::MacroDuplicateParameter { name, parameter } => Some(format!(
                "macro `{name}` has duplicate parameter `{parameter}` — rename one"
            )),

            // ── Query diagnostics ──
            Self::QueryBoundsExceeded { max_nodes, max_depth } => Some(format!(
                "query exceeded bounds (maxNodes={max_nodes}, maxDepth={max_depth}) — narrow the query scope"
            )),
            Self::QueryNodeSpanUnavailable => Some(
                "the node's span is unavailable in this syntax generation — reparse the source".to_string()
            ),
            Self::QueryPipelineConflict => Some(
                "the pipeline contains conflicting operations — reorder or remove duplicates".to_string()
            ),
            Self::QueryPipelineStaleGeneration => Some(
                "the pipeline references a stale syntax generation — refresh the snapshot".to_string()
            ),

            // ── Composition diagnostics ──
            Self::CompositionMissingLaunchHost => Some(
                "add exactly one `launch` host entry point to the composition root".to_string()
            ),
            Self::CompositionMultipleLaunchHosts => Some(
                "remove all but one `launch` host entry point".to_string()
            ),
            Self::CompositionDependencyCycle { from_id, to_id } => Some(format!(
                "break the cycle between registrations {from_id} and {to_id}"
            )),
            Self::CompositionUnresolvedInject { requested_type } => Some(format!(
                "register a binding for type `{requested_type}` or check the inject signature"
            )),
            Self::CompositionAmbiguousInject { requested_type } => Some(format!(
                "multiple bindings provide `{requested_type}` — qualify the inject or remove duplicates"
            )),
            Self::CompositionScopedOutsideWith => Some(
                "wrap the scoped resolution in a `with` block".to_string()
            ),
            Self::CompositionChildScopeWithoutParent { scope_name } => Some(format!(
                "register scope `{scope_name}` as a child of an existing scope"
            )),
            Self::CompositionWithArgsMismatch { scope_name } => Some(format!(
                "match the argument list of `with {scope_name}(...)` to the scope definition"
            )),
            Self::CompositionLaunchTargetNotHost { target_name } => Some(format!(
                "`{target_name}` must be a host definition — add `host` to the declaration"
            )),
            Self::CompositionHostInheritanceCycle { host_name } => {
                Some(format!("break the inheritance cycle at `{host_name}`"))
            }
            Self::CompositionDuplicateScopeName { scope_name } => Some(format!(
                "rename one of the duplicate scope `{scope_name}` declarations"
            )),
            Self::CompositionHostInModProject => Some(
                "move the host definition to an application project, not a mod project".to_string()
            ),
            Self::CompositionLaunchInLibProject => Some(
                "move the `launch` to an application project, not a library project".to_string()
            ),
            Self::CompositionInjectOnConstructor => Some(
                "move `inject` fields off the constructor — use a scope or factory instead".to_string()
            ),
            Self::CompositionOverrideLifetimeMismatch { binding } => Some(format!(
                "the override for `{binding}` must match the original lifetime"
            )),
            Self::CompositionInvalidScopeQualifier { qualifier } => Some(format!(
                "use a valid scope qualifier instead of `{qualifier}`"
            )),

            // ── Documentation diagnostics ──
            Self::DocUnknownArgName { name } => {
                Some(format!("rename `@arg({name})` to match a parameter or remove it"))
            }
            Self::DocDuplicateArgName { name } => {
                Some(format!("remove the duplicate `@arg({name})`"))
            }
            Self::DocArgOrReturnsOnNonCallable => Some(
                "move `@arg`/`@returns` to a function, method, or contract method".to_string()
            ),
            Self::DocReturnsOnUnit => Some("remove `@returns` — the callable returns `unit`".to_string()),
            Self::DocUnknownDirective { name } => Some(format!(
                "remove `@{name}` or use a known directive: @arg, @returns, @ref, @variant, @par"
            )),
            Self::DocUnresolvedRef { .. } => {
                Some("use a name that exists in this compilation unit".to_string())
            }
            Self::DocVariantOnNonEnum => Some(
                "move `@variant(...)` to an `enum` declaration's leading docs".to_string()
            ),
            Self::DocUnknownVariantName { name } => {
                Some(format!("rename `@variant({name})` to match an enum variant or remove it"))
            }
            Self::DocDuplicateVariantName { name } => {
                Some(format!("remove the duplicate `@variant({name})`"))
            }
            Self::DocParWithoutGenerics => Some(
                "add generic type parameters to the declaration or remove `@par`".to_string()
            ),
            Self::DocUnknownGenericName { name } => {
                Some(format!("rename `@par({name})` to match a generic parameter or remove it"))
            }
            Self::DocDuplicateGenericName { name } => {
                Some(format!("remove the duplicate `@par({name})`"))
            }
            Self::RedundantEnumConstructorParens => {
                Some("remove the redundant empty parentheses".to_string())
            }

            // ── Naming-style warnings (W1630–W1638) ──
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
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::LowerCamelCase)
            )),
            Self::NamingNotSnakeCaseTest { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::SnakeCase)
            )),
        }
    }
}
