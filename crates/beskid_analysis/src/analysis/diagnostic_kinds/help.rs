use super::{DiscardedGrowthRepair, SemanticIssueKind};

impl SemanticIssueKind {
    pub fn help(&self) -> Option<String> {
        let text = self.help_text();
        if self.names_module_path() { text.map(|text| text.replace("::", ".")) } else { text }
    }

    fn help_text(&self) -> Option<String> {
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
            Self::UnknownImportPath { path, module: Some(module) } => {
                let item = path.rsplit('.').next().unwrap_or(path);
                Some(format!("`use` imports modules; write `use {module};` and refer to `{module}.{item}`"))
            }
            Self::UnknownImportPath { path, module: None } => {
                Some(format!("check the module path `{path}` — it may not exist or may not be exported"))
            }
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
            Self::AttributeTargetNotAllowed { attribute, declared: false, .. } => Some(format!(
                "no attribute declaration named `{attribute}` exists; declare it with `attribute {attribute}(<targets>) {{ }}` or import its module"
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
            Self::DuplicateFileScopedModule { .. } => Some(
                "a file declares its own module once with `mod X;` at the top; other files of the module do not \
                 repeat it"
                    .to_string(),
            ),
            Self::ModuleDeclarationForbiddenInFileScopedModule => {
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
            Self::UnusedPrivateItem { name, .. } => Some(format!(
                "remove `{name}`, or mark it `pub` if other modules need it"
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
            Self::ContractAssociatedTypeMissingBinding { contract_name, assoc_name } => Some(format!(
                "declare `type {assoc_name} = <concrete type>;` in the implementor, or add a default to `{contract_name}`'s `type {assoc_name};`"
            )),
            Self::ThisUsedOutsideContractOrImpl => {
                Some("`This` is only valid inside a contract's own method signatures or an impl block for a concrete receiver".to_string())
            }
            Self::UnresolvedAssociatedType { .. } => {
                Some("check the contract name and that the referenced type conforms to it".to_string())
            }
            Self::GenericBoundNotSatisfied { type_name, contract_name, .. }
                if super::type_names::outer_name(&super::display_type(contract_name)) == "Serializable" =>
            {
                Some(format!(
                    "add `[Serialize]` to type `{}` so the compiler derives its `Serializable` conformance",
                    super::display_type(type_name)
                ))
            }
            Self::GenericBoundNotSatisfied { reason: Some(reason), .. } => Some(reason.clone()),
            Self::GenericBoundNotSatisfied { type_name, contract_name, reason: None, .. } => {
                Some(format!("make `{type_name}` conform to `{contract_name}` or pass a conforming type"))
            }
            Self::InvalidPrimitiveImplementation { reason, .. } => Some(reason.clone()),
            // The declaration text is needed for the tailored help; `issue_diagnostic` writes it.
            Self::ImmutableAssignment { name, binding } => Some(match binding.form {
                super::BindingForm::ForIterator => {
                    format!("a `for` iterator cannot be assigned; copy `{name}` into a `mut` local first")
                }
                _ => format!("declare `{name}` with `mut`"),
            }),

            // ── Pattern / match / control-flow diagnostics ──
            Self::MatchGuardMustBeBoolean => {
                Some("the match guard expression must evaluate to `bool`".to_string())
            }
            Self::MatchArmTypeMismatch { expected, actual } => {
                Some(format!("expected `{expected}`, got `{actual}`"))
            }
            Self::MatchNonExhaustive { enum_name, missing_pattern } => Some(format!(
                "add an arm matching `{missing_pattern}`, cover every variant of `{enum_name}`, or add a wildcard `_` arm"
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
            Self::ResolveUnknownValue { suggestion: Some(suggestion), .. } => {
                Some(format!("a name with a similar spelling exists: `{suggestion}`"))
            }
            Self::ResolveUnknownValue { name, suggestion: None } => Some(format!(
                "check the spelling of `{name}` or add a `use` import if it's defined elsewhere"
            )),
            Self::ResolveUnknownType { name } => Some(format!(
                "check the spelling of `{name}` or add a `use` import if it's defined elsewhere"
            )),
            Self::ResolveUnknownModulePath { path } => {
                Some(format!("check the module path `{path}` — it may not exist or may not be in scope"))
            }
            Self::ResolveUnknownValueInModule { module_path, name } => Some(format!(
                "check that module `{module_path}` exports a value named `{name}`"
            )),
            Self::ResolveUnknownTypeInModule { module_path, name } => Some(format!(
                "check that module `{module_path}` exports a type named `{name}`"
            )),
            Self::ResolveInvalidConformanceTarget { .. } => Some(
                "declare conformance against a `contract`, not a type/enum/function".to_string()
            ),
            Self::ResolvePrivateRuntimeBuiltin { .. } => Some(
                "call the public Corelib API instead; only compiler-admitted Corelib sources may call runtime builtins".to_string()
            ),
            Self::ResolvePrivateItemInModule { .. } => {
                Some("mark the item `pub` or avoid cross-module access".to_string())
            }
            Self::ResolveAmbiguousModulePath { path, .. } => Some(format!(
                "module `{path}` must be declared by exactly one package of the dependency closure"
            )),
            Self::ResolveAmbiguousItemInModule { module_path, name, .. } => Some(format!(
                "qualify `{name}` through the child module that declares it; `{module_path}` re-exports more than one"
            )),
            Self::ResolveAmbiguousModuleQualifier { import_path, .. } => Some(format!(
                "rename the inline module, or import the module under another name: `use {import_path} as <Name>;`"
            )),
            Self::MissingImport { name, module_path } => {
                Some(format!("add `use {module_path}.{name};` at the top of the file"))
            }
            Self::MissingImportAmbiguous { name, candidates } => Some(format!(
                "add one of: {}",
                candidates
                    .iter()
                    .map(|c| format!("`use {c}.{name};`"))
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
                "the type could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeInvalidMemberTarget => Some(
                "member access requires a `type` with fields — check the receiver type".to_string()
            ),
            Self::TypeUnknownEnumType => Some(
                "the enum type could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeUnknownStructField { suggestion: Some(suggestion), .. } => {
                Some(format!("a field with a similar name exists: `{suggestion}`"))
            }
            Self::TypeUnknownStructField { fields, .. } if !fields.is_empty() => Some(format!(
                "its fields are: {}",
                fields.iter().map(|field| format!("`{field}`")).collect::<Vec<_>>().join(", ")
            )),
            Self::TypeUnknownStructField { type_name: Some(type_name), .. } => {
                Some(format!("`{}` declares no fields", super::display_type(type_name)))
            }
            Self::TypeUnknownStructField { name, .. } => {
                Some(format!("check that the type declares a field named `{name}`"))
            }
            Self::TypeInaccessibleStructField { name } => Some(format!(
                "`{name}` is private to the source unit that declares its type; mark the field `pub` in \
                 its declaring type, or use that type's public API"
            )),
            Self::TypeUnknownEnumVariant { name } => {
                Some(format!("check that the enum has a variant named `{name}`"))
            }
            Self::TypeMissingStructField { name } => {
                Some(format!("add `{name}: <value>` to the type literal"))
            }
            Self::TypeMissingTypeAnnotation { name } => Some(format!(
                "add an explicit type annotation for `{name}`, e.g. `let {name}: i32 = ...`"
            )),
            Self::TypeMissingTypeArguments { parameter: None } => {
                Some("provide explicit type arguments, e.g. `Type<i32>`".to_string())
            }
            Self::TypeMissingTypeArguments { parameter: Some(parameter) } => Some(format!(
                "write the type arguments at the call, e.g. `.Name<i64>(...)`, to bind `{parameter}`"
            )),
            Self::TypeGenericArgumentMismatch { expected, actual } => Some(format!(
                "the generic type expects {expected} argument(s) but got {actual}"
            )),
            Self::TypeGenericParameterConflict { parameter, first_name, second_name } => Some(format!(
                "an earlier argument fixed `{parameter}` to `{first_name}`, but this argument is `{second_name}` — \
                 use matching declared types, or an explicit conversion, for every use of `{parameter}`"
            )),
            Self::TypeMismatch { expected_name, actual_name }
            | Self::TypeReturnMismatch { expected_name, actual_name }
            | Self::TypeCallArgumentMismatch { expected_name, actual_name } => {
                Some(mismatch_help(expected_name, actual_name))
            }
            Self::TypeMatchArmMismatch { expected_name, actual_name } => {
                let (expected, actual) = super::display_type_pair(expected_name, actual_name);
                Some(format!("make every arm produce `{expected}`; this arm produces `{actual}`"))
            }
            Self::TypeCallArityMismatch { required, total, actual } => Some(if required == total {
                format!("pass exactly {total} argument(s); this call passes {actual}")
            } else {
                format!(
                    "pass the {required} required argument(s) and up to {} optional one(s); this call passes {actual}",
                    total - required
                )
            }),
            Self::RequiredParameterAfterOptional { name } => Some(format!(
                "optional parameters (`= value`) come after every required parameter; move `{name}` before them or give it a default value"
            )),
            Self::DefaultValueNotConstant { .. } => Some(
                "a default value is a literal (`0`, `1.5`, `true`, `'c'`, `\"text\"`), a negated number, or the name of a `const` declared in this file"
                    .to_string(),
            ),
            Self::DefaultValueTypeMismatch { name, expected, .. } => {
                Some(format!("give `{name}` a constant of type `{expected}`, or change the parameter type"))
            }
            Self::OptionalParameterNotAllowed { place } if place.contains("host or scope") => Some(
                "remove the caller-information attribute; a `launch` or `with` may still omit the argument when the parameter has a default value"
                    .to_string(),
            ),
            Self::OptionalParameterNotAllowed { .. } => Some(
                "no call passes this argument, so neither a default value nor a caller-information attribute can apply; remove it"
                    .to_string(),
            ),
            Self::CallerInformationWithoutDefault { name, expected, .. } => Some(format!(
                "write `{expected} {name} = {}`; a call that omits the argument gets the call site's value",
                if expected == "string" { "\"\"" } else { "0" }
            )),
            Self::ContractDefaultValueMismatch { parameter, .. } => Some(format!(
                "a contract method's default values belong to the contract; remove the default of `{parameter}` from the implementation (it inherits the contract's) or make it equal"
            )),
            Self::CallerInformationParameterType { attribute, expected, .. } => {
                Some(format!("`[{attribute}]` fills a `{expected}`; declare the parameter as `{expected}`"))
            }
            Self::TypeEnumConstructorMismatch { expected, actual } => Some(format!(
                "the enum constructor expects {expected} argument(s) but got {actual}"
            )),
            Self::TypeUnknownCallTarget => Some(
                "the call target could not be resolved — check spelling and imports".to_string()
            ),
            Self::TypeInvalidBinaryOp => Some(
                "the binary operator is not valid for these operand types".to_string()
            ),
            Self::TypeStringConcatOperandNotConvertible { .. } => Some(
                "only string, bool, f32, f64, i8, i16, i32, i64, u8, u16 and u32 values convert to text; \
                 produce a string from the value first"
                    .to_string(),
            ),
            Self::TypeInvalidCompoundAssignment { operator, type_name } => Some(match operator.as_str() {
                "%=" => format!(
                    "`%` works on integer types only; convert `{}` to an integer type first",
                    super::display_type(type_name)
                ),
                _ => format!("`{operator}` needs a numeric target (`+=` also accepts `string`)"),
            }),
            Self::TypeInvalidUnaryOp => Some(
                "the unary operator is not valid for this operand type".to_string()
            ),
            Self::TypeNonBoolCondition => Some("the condition must evaluate to `bool`".to_string()),
            Self::TypeMissingReturn { result_name, .. } => Some(format!(
                "end every path with `return <value>;` of type `{}`, or with a statement that does not continue \
                 (`while (true)` without `break`, or a call that returns `never`)",
                super::display_type(result_name)
            )),
            Self::TypeUnsupportedExpression => Some(
                "this expression form is not supported in the current context".to_string()
            ),
            Self::TypeInvalidTryTarget => {
                Some("apply `?` only to a `Result<Ok, Error>` expression".to_string())
            }
            Self::TypeInvalidPrimitiveConversionArgument => Some(
                "primitive conversions like `i32(x)` accept exactly one primitive numeric argument".to_string(),
            ),
            Self::ScopedCleanupRejected { .. } => Some(
                "a scoped `use` needs a fresh disposable resource whose `Dispose` returns a `Result` convertible to \
                 the enclosing function's error, and the resource must not escape or be disposed explicitly"
                    .to_string(),
            ),
            Self::InternalSemanticFactUnavailable { .. }
            | Self::InternalLoweringRuleMissing { .. }
            | Self::InternalLoweringInvariant { .. }
            | Self::InternalModSemanticFailure { .. }
            | Self::InternalBuildFailure { .. } => self.internal_help(),
            Self::CopyOnAssignWithoutCopy { type_name, .. } => Some(format!(
                "declare `pub This Copy() {{ ... }}` in `{type_name}`: a borrowed receiver, no parameters, returning \
                 a `{type_name}` with its own storage"
            )),
            Self::DiscardedArrayGrowth { call, repair, .. } => Some(match repair {
                DiscardedGrowthRepair::MutParameter { name, .. } => {
                    format!("declare the parameter `mut` (`mut ... {name}`) so the call grows the caller's array")
                }
                DiscardedGrowthRepair::Assign { name, .. } => format!("assign the result: `{name} = {call};`"),
                DiscardedGrowthRepair::Bind { declaration: Some(declaration), .. } => {
                    format!("use the result: `{declaration} grown = {call};`")
                }
                DiscardedGrowthRepair::Bind { declaration: None, .. } => {
                    "use the result: bind it to a new local".to_string()
                }
            }),
            Self::DeadParameterAssignment { parameter } => Some(format!(
                "parameters pass by value, so the caller does not see this assignment: return the new value, or \
                 assign the fields of the caller's object (`{parameter}.<field> = ...`)"
            )),
            Self::DeadCollectionGrowth => {
                Some("return the parameter or rebind the grown handle so the caller sees the growth".to_string())
            }
            Self::LambdaCapturesStackReference { .. } => Some(
                "lambdas capture values, not storage: copy the value into an immutable local and capture that"
                    .to_string(),
            ),
            Self::UseAfterMove { name } => {
                Some(format!("`{name}` was consumed by the move; reassign it before using it again"))
            }
            Self::CapturedLocalAssignedInBoundLambda { .. } => Some("use a reduction".to_string()),
            Self::CaptureNotTransferable { .. } => Some(
                "capture a Shared or Copied value, or `move` a Moved one, as the `[Captures]` bound requires"
                    .to_string(),
            ),
            Self::MovedCaptureWithoutMove { name } | Self::ImplicitAliasOfMovedValue { name } => {
                Some(format!("write `move {name}`"))
            }
            Self::CapturesArgumentNotLiteral => Some("pass the lambda literal".to_string()),
            Self::BorrowedParameterKept { .. } => Some("copy it, or declare the parameter `move`".to_string()),
            Self::NestedCopyOnAssignCrossing { type_name, .. } => Some(format!(
                "copy it first: build the value from fresh `{type_name}` values, or send the `{type_name}` on its own"
            )),
            Self::ScopeBorrowedEscapes { .. } => {
                Some("use the value inside the scope that created it: read it or pass it to a call".to_string())
            }
            // A place read at a crossing stays reachable from its root.
            Self::BorrowEscapes { name } if name.contains('[') => Some(format!(
                "copy `{name}` first, or take it out with `exchange({name}, <value>)`"
            )),
            Self::BorrowEscapes { name } if name.contains('.') => {
                Some(format!("copy `{name}` first, or take it out with `move {name}`"))
            }
            Self::BorrowEscapes { name } => {
                Some(format!("copy `{name}` first: another name in this callable can still reach its value"))
            }
            Self::MoveOfNonOwningName { class: None, .. } => {
                Some("remove `move`; it applies only when a value crosses to another isolate".to_string())
            }
            Self::MoveOfNonOwningName { class: Some(_), .. } => {
                Some("remove `move`: Shared and Copied values pass without it".to_string())
            }
            Self::MoveInLoopWithoutReassignment { name } => {
                Some(format!("assign `{name}` a new value before the end of the loop body"))
            }
            Self::DestructiveReadWithoutEmpty { .. } => Some("use `exchange(<place>, <value>)`".to_string()),
            Self::ImmutableFieldNotShared { .. } => {
                Some("declare the field with a Shared type, such as a primitive, string or `[Immutable]` type".to_string())
            }
            Self::ImmutableFieldAssignment { .. } => {
                Some("set the field in the constructing literal and build a new value to change it".to_string())
            }
            Self::MarkerOnNeverTransferable { .. } => Some(
                "remove the marker: values of this type cannot leave their runtime instance".to_string(),
            ),
            Self::MarkerFieldClassMismatch { .. } => Some(
                "`[Isolated]` admits Moved, Shared and Copied fields; `[Copyable]` admits Shared and Copied fields"
                    .to_string(),
            ),
            Self::CapturesBoundNotMarker { .. } => Some(
                "bound `[Captures]` with a marker contract such as `Transferable`, on a function-typed parameter"
                    .to_string(),
            ),
            Self::InjectInsideIsolateBody { field } => Some(format!(
                "read `{field}` before the task starts and capture the value instead"
            )),
            Self::GenericReceiverMethodUndeclared { method, parameter } => Some(format!(
                "add a `where {parameter}: Contract` bound whose contract declares `{method}`"
            )),
            Self::EnumValueMethodCall { .. } => Some("unwrap with `match` or `?` first".to_string()),
            Self::CompilerDerivedConformance { contract } => {
                Some(format!("remove `{contract}` from the conformance list and use a marker attribute on the type"))
            }
            Self::CompilerDerivedNotMarker { .. } => Some("remove `[CompilerDerived]`".to_string()),
            Self::DerivedContractMember { .. } => Some("a compiler-derived contract has no members".to_string()),
            Self::ExclusiveMarkerAttribute { .. } => {
                Some("keep one of `[Immutable]`, `[Isolated]` and `[Copyable]`".to_string())
            }
            Self::CorelibOnlyAttribute { .. } => Some("remove the attribute".to_string()),
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
            | Self::NamingNotPascalCaseGeneric { name }
            | Self::NamingNotPascalCaseTest { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::PascalCase)
            )),
            Self::NamingNotCamelCaseField { name }
            | Self::NamingNotCamelCaseBinding { name }
            | Self::NamingNotCamelCaseMacro { name } => Some(format!(
                "try `{}`",
                crate::naming_case::normalize_to_profile(name, crate::naming_case::NamingProfile::LowerCamelCase)
            )),
            Self::NumericLiteralOutOfRange { value, primitive } => Some(numeric_range_help(value, primitive)),
            Self::ExternInvalidAbi { .. } => {
                Some("Use [Extern(Abi:\"C\", Library:\"...\")] on the contract".to_string())
            }
            Self::ExternMissingLibrary => Some("Provide Library:\"<soname>\"; e.g., libc.so.6 on Linux".to_string()),
            Self::ExternDisallowedParamType { detail, .. }
            | Self::ExternDisallowedReturnType { detail, .. }
            | Self::GlueBindingRejected { detail, .. }
            | Self::ExternInvalidOptional { detail }
            | Self::InvalidClifBlock { detail } => Some(detail.clone()),
        }
    }
}

/// Help for a value of type `actual` where `expected` is required. A `Result` or `Option` meeting
/// another type is unwrapped with `?` or `match`, never cast; a cast is suggested only between
/// numeric primitives.
fn mismatch_help(expected_name: &str, actual_name: &str) -> String {
    let (expected, actual) = super::display_type_pair(expected_name, actual_name);
    let (expected_outer, actual_outer) =
        (super::type_names::outer_name(&expected), super::type_names::outer_name(&actual));
    match actual_outer {
        "Result" if expected_outer != "Result" => {
            return "add `?` to propagate the error, or `match` on the result".to_string();
        }
        "Option" if expected_outer != "Option" => {
            return "add `?` to propagate `None`, or `match` on the option".to_string();
        }
        _ => {}
    }
    match expected_outer {
        "Result" if actual_outer != "Result" => {
            return format!("wrap the value: `Result::Ok(...)` produces `{expected}`");
        }
        "Option" if actual_outer != "Option" => {
            return format!("wrap the value: `Option::Some(...)` produces `{expected}`");
        }
        _ => {}
    }
    if super::type_names::is_numeric_primitive(&expected) && super::type_names::is_numeric_primitive(&actual) {
        format!("convert the value with `{expected}(...)`")
    } else {
        format!("this expression has type `{actual}`, but `{expected}` is required here")
    }
}

/// The range help of T0905: what `primitive` holds, and for a negative value at an unsigned
/// type that it holds no negative value.
fn numeric_range_help(value: &str, primitive: &str) -> String {
    let range = match primitive {
        "i8" => Some((i8::MIN.to_string(), i8::MAX.to_string())),
        "i16" => Some((i16::MIN.to_string(), i16::MAX.to_string())),
        "i32" => Some((i32::MIN.to_string(), i32::MAX.to_string())),
        "i64" => Some((i64::MIN.to_string(), i64::MAX.to_string())),
        "u8" => Some(("0".to_string(), u8::MAX.to_string())),
        "u16" => Some(("0".to_string(), u16::MAX.to_string())),
        "u32" => Some(("0".to_string(), u32::MAX.to_string())),
        "u64" | "word" => Some(("0".to_string(), u64::MAX.to_string())),
        _ => None,
    };
    match range {
        Some((low, _)) if low == "0" && value.starts_with('-') => {
            format!("`{primitive}` holds no negative values; use a signed type such as `i64`")
        }
        Some((low, high)) => {
            format!("`{primitive}` holds values from {low} to {high}; use a value in that range or a wider type")
        }
        None => format!("`{primitive}` holds only finite values; use a smaller value or `f64`"),
    }
}

#[cfg(test)]
mod numeric_range_help_tests {
    use super::super::SemanticIssueKind;

    #[test]
    fn out_of_range_literal_names_the_value_the_type_and_its_range() {
        let issue =
            SemanticIssueKind::NumericLiteralOutOfRange { value: "300".to_string(), primitive: "u8".to_string() };
        assert_eq!(issue.code(), "T0905");
        assert_eq!(issue.message(), "`300` does not fit in `u8`");
        assert_eq!(issue.label(), "out of range for `u8`");
        assert_eq!(
            issue.help().as_deref(),
            Some("`u8` holds values from 0 to 255; use a value in that range or a wider type")
        );
    }

    #[test]
    fn negative_literal_at_an_unsigned_type_says_it_holds_no_negative_values() {
        let issue =
            SemanticIssueKind::NumericLiteralOutOfRange { value: "-1".to_string(), primitive: "u32".to_string() };
        assert_eq!(issue.message(), "`-1` does not fit in `u32`");
        assert_eq!(issue.help().as_deref(), Some("`u32` holds no negative values; use a signed type such as `i64`"));
    }

    #[test]
    fn float_out_of_range_says_the_type_holds_finite_values() {
        let issue =
            SemanticIssueKind::NumericLiteralOutOfRange { value: "1e40".to_string(), primitive: "f32".to_string() };
        assert_eq!(issue.message(), "`1e40` does not fit in `f32`");
        assert_eq!(issue.help().as_deref(), Some("`f32` holds only finite values; use a smaller value or `f64`"));
    }
}
