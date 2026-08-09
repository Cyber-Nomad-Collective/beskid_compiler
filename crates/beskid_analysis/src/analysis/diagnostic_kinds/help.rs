use super::SemanticIssueKind;

impl SemanticIssueKind {
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
