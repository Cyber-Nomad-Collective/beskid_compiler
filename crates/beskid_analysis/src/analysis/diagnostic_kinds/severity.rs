use super::SemanticIssueKind;
use crate::analysis::Severity;

impl SemanticIssueKind {
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
}
