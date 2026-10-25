//! Declaration identity for composition. Registry keys, `inject` requests, overrides and the
//! registration shape checks compare the declarations their paths name, never their spelling:
//! `A.Clock` and `B.Clock` are different keys, and `Hosting.CommandLine` and `CommandLine` (through
//! `use`) are one key when they name one declaration. A generic contract key carries its type
//! arguments: `Repo<User>` and `Repo<Order>` are different keys, `Repo<User>` twice is one.
//!
//! The only authority that resolves a type path (lexical scope, `use` imports, module paths) is the
//! query layer (`beskid_queries`), which the build pipeline and the editor hand to composition as a
//! [`CompositionTypeResolver`]. There is no name-based fallback: a pipeline without the resolver
//! does not run composition at all.

use crate::syntax::{Path, SpanInfo, Spanned, Type};

use super::model::UnitPath;

/// One type, contract or enum of the assembly: the unit that declares it (`None`: the unit under
/// analysis), the span of its name, and the type arguments applied to it (empty for a
/// non-generic use and for the declaration itself).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeIdentity {
    pub unit: UnitPath,
    pub declaration: SpanInfo,
    pub arguments: Vec<TypeArgument>,
}

impl TypeIdentity {
    /// The declaration itself: `Repo` for `Repo<User>`.
    pub fn declaration_only(&self) -> Self {
        Self { unit: self.unit.clone(), declaration: self.declaration, arguments: Vec::new() }
    }
}

/// One type argument of an applied generic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeArgument {
    Declared(TypeIdentity),
    /// A primitive type, by its source spelling (`i64`, `string`).
    Primitive(String),
    Array(Box<TypeArgument>),
}

/// Resolves a type path to the declaration it names.
pub trait CompositionTypeResolver {
    /// The declaration that `path`, written at `site` (the span of the path) in `unit`, names,
    /// with no `arguments`: the type arguments of the path only select a generic declaration of
    /// that arity ([`path_identity`] resolves them). `None` when it names no unique type,
    /// contract or enum.
    fn resolve_type(&self, unit: &UnitPath, site: SpanInfo, path: &Path) -> Option<TypeIdentity>;
}

/// The identity of `path` written in `unit`: its declaration with its type arguments resolved in
/// the same unit. `None` when the declaration or any argument does not resolve.
pub fn path_identity(
    types: &dyn CompositionTypeResolver,
    unit: &UnitPath,
    path: &Spanned<Path>,
) -> Option<TypeIdentity> {
    let declaration = types.resolve_type(unit, path.span, &path.node)?;
    let arguments = path
        .node
        .segments
        .last()?
        .node
        .type_args
        .iter()
        .map(|argument| argument_identity(types, unit, argument))
        .collect::<Option<Vec<_>>>()?;
    Some(TypeIdentity { arguments, ..declaration })
}

fn argument_identity(types: &dyn CompositionTypeResolver, unit: &UnitPath, ty: &Spanned<Type>) -> Option<TypeArgument> {
    match &ty.node {
        Type::Primitive(primitive) => Some(TypeArgument::Primitive(format!("{:?}", primitive.node))),
        Type::Complex(path) => path_identity(types, unit, path).map(TypeArgument::Declared),
        Type::Array(inner) => argument_identity(types, unit, inner).map(|inner| TypeArgument::Array(Box::new(inner))),
        Type::Associated { .. } | Type::This | Type::Function { .. } => None,
    }
}

#[cfg(test)]
pub(crate) mod test_types {
    //! A resolver for this crate's unit tests, which have no query database: a path names the
    //! unique type, contract or enum declaration (at any inline-module depth) whose name is its
    //! last segment. Module qualification is not checked here; `beskid_queries` tests cover it.

    use std::collections::HashMap;

    use super::{CompositionTypeResolver, TypeIdentity};
    use crate::composition::model::UnitPath;
    use crate::syntax::{Node, Path, Program, SpanInfo, Spanned};

    pub(crate) struct DeclaredTypes {
        declarations: HashMap<String, Vec<TypeIdentity>>,
    }

    impl DeclaredTypes {
        pub(crate) fn new(
            entry: &Spanned<Program>,
            imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
        ) -> Self {
            let mut declarations = HashMap::new();
            collect(&entry.node.items, &None, &mut declarations);
            for (path, program) in imported {
                collect(&program.node.items, &Some(std::sync::Arc::clone(path)), &mut declarations);
            }
            Self { declarations }
        }
    }

    fn collect(items: &[Spanned<Node>], unit: &UnitPath, found: &mut HashMap<String, Vec<TypeIdentity>>) {
        for item in items {
            let name = match &item.node {
                Node::TypeDefinition(definition) => &definition.node.name,
                Node::ContractDefinition(contract) => &contract.node.name,
                Node::EnumDefinition(definition) => &definition.node.name,
                Node::InlineModule(module) => {
                    collect(&module.node.items, unit, found);
                    continue;
                }
                _ => continue,
            };
            found.entry(name.node.name.clone()).or_default().push(TypeIdentity {
                unit: unit.clone(),
                declaration: name.span,
                arguments: Vec::new(),
            });
        }
    }

    impl CompositionTypeResolver for DeclaredTypes {
        fn resolve_type(&self, _unit: &UnitPath, _site: SpanInfo, path: &Path) -> Option<TypeIdentity> {
            let name = &path.segments.last()?.node.name.node.name;
            match self.declarations.get(name)?.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            }
        }
    }
}
