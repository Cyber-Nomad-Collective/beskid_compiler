//! The closed attribute target catalogue and the compiler-recognized attributes.
//!
//! Target names are the PascalCase names of the grammar productions that accept an
//! `AttributeList` (`beskid.pest`). `MethodDefinition` denotes `ImplMethodDefinition` (in `impl`,
//! `extend type` and inline type bodies) and `ModuleDeclaration` denotes both
//! `ModuleDeclaration` and `InlineModule`. The order of [`ATTRIBUTE_TARGET_CATALOGUE`] is the
//! order the E1509 help text and the LSP completion list use.

use crate::syntax_query::NodeKind;

/// Every attribute target name, in catalogue order.
pub const ATTRIBUTE_TARGET_CATALOGUE: &[&str] = &[
    "ModuleDeclaration",
    "TypeDefinition",
    "EnumDefinition",
    "EnumVariant",
    "ContractDefinition",
    "ContractMethodSignature",
    "ContractAssociatedType",
    "ContractEmbedding",
    "ImplBlock",
    "FunctionDefinition",
    "MethodDefinition",
    "ReceiverParameter",
    "Parameter",
    "LambdaExpression",
    "LambdaParameter",
    "TypeParameter",
    "Field",
    "LetStatement",
    "ConstantDefinition",
    "TestDefinition",
    "AttributeDeclaration",
];

/// Whether `name` is a target of the closed catalogue.
pub fn is_attribute_target(name: &str) -> bool {
    ATTRIBUTE_TARGET_CATALOGUE.contains(&name)
}

/// The catalogue as owned strings (E1509 and E1510 help text).
pub fn attribute_target_catalogue() -> Vec<String> {
    ATTRIBUTE_TARGET_CATALOGUE.iter().map(|name| (*name).to_string()).collect()
}

/// The target name of the syntax node that owns an attribute list, or `None` when the node kind
/// carries no attributes.
pub fn attribute_site_target(kind: NodeKind) -> Option<&'static str> {
    Some(match kind {
        NodeKind::ModuleDeclaration | NodeKind::InlineModule => "ModuleDeclaration",
        NodeKind::TypeDefinition => "TypeDefinition",
        NodeKind::EnumDefinition => "EnumDefinition",
        NodeKind::EnumVariant => "EnumVariant",
        NodeKind::ContractDefinition => "ContractDefinition",
        NodeKind::ContractMethodSignature => "ContractMethodSignature",
        NodeKind::ContractAssociatedType => "ContractAssociatedType",
        NodeKind::ContractEmbedding => "ContractEmbedding",
        NodeKind::ImplBlock => "ImplBlock",
        NodeKind::FunctionDefinition => "FunctionDefinition",
        NodeKind::MethodDefinition => "MethodDefinition",
        NodeKind::ReceiverParameter => "ReceiverParameter",
        NodeKind::Parameter => "Parameter",
        NodeKind::LambdaExpression => "LambdaExpression",
        NodeKind::LambdaParameter => "LambdaParameter",
        NodeKind::TypeParameter => "TypeParameter",
        NodeKind::Field => "Field",
        NodeKind::LetStatement => "LetStatement",
        NodeKind::ConstantDefinition => "ConstantDefinition",
        NodeKind::TestDefinition => "TestDefinition",
        NodeKind::AttributeDeclaration => "AttributeDeclaration",
        _ => return None,
    })
}

/// An attribute the compiler recognizes by name, with the target list of its corelib
/// declaration in [`RECOGNIZED_ATTRIBUTE_MODULE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecognizedAttribute {
    pub name: &'static str,
    pub targets: &'static [&'static str],
}

/// The corelib module that declares every compiler-recognized attribute.
pub const RECOGNIZED_ATTRIBUTE_MODULE: &[&str] = &["Core", "Attributes"];

/// Every attribute the compiler recognizes by name. The corelib declarations in
/// `Core.Attributes` carry exactly these target lists (checked by a corelib agreement test), so
/// the list is enforced even in an assembly that does not include that unit.
pub const COMPILER_RECOGNIZED_ATTRIBUTES: &[RecognizedAttribute] = &[
    RecognizedAttribute { name: "Export", targets: &["FunctionDefinition", "MethodDefinition"] },
    RecognizedAttribute { name: "Extern", targets: &["ContractDefinition"] },
    RecognizedAttribute { name: "Runtime", targets: &["FunctionDefinition"] },
    RecognizedAttribute { name: "CompilerDerived", targets: &["ContractDefinition"] },
    RecognizedAttribute { name: "Immutable", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "Isolated", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "Copyable", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "NeverTransferable", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "ImmutableStorage", targets: &["Field"] },
    RecognizedAttribute { name: "ScopeBorrowed", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "Captures", targets: &["Parameter", "Field"] },
    RecognizedAttribute { name: "GlueHandle", targets: &["TypeDefinition"] },
    RecognizedAttribute { name: "RustOwner", targets: &["TypeDefinition", "ContractMethodSignature"] },
    RecognizedAttribute { name: "CleanupConversion", targets: &["FunctionDefinition"] },
];

/// The recognized attribute named `name`, if any.
pub fn recognized_attribute(name: &str) -> Option<&'static RecognizedAttribute> {
    COMPILER_RECOGNIZED_ATTRIBUTES.iter().find(|attribute| attribute.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_names_are_unique_and_every_site_maps_into_it() {
        let mut names = ATTRIBUTE_TARGET_CATALOGUE.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ATTRIBUTE_TARGET_CATALOGUE.len());
        for kind in NodeKind::ALL {
            if let Some(target) = attribute_site_target(*kind) {
                assert!(is_attribute_target(target), "{kind:?} maps to {target}");
            }
        }
        for target in ATTRIBUTE_TARGET_CATALOGUE {
            assert!(
                NodeKind::ALL.iter().any(|kind| attribute_site_target(*kind) == Some(target)),
                "no syntax node carries target {target}"
            );
        }
    }

    #[test]
    fn recognized_attributes_name_catalogue_targets() {
        for attribute in COMPILER_RECOGNIZED_ATTRIBUTES {
            assert!(!attribute.targets.is_empty(), "{}", attribute.name);
            for target in attribute.targets {
                assert!(is_attribute_target(target), "{} lists {target}", attribute.name);
            }
        }
    }
}
