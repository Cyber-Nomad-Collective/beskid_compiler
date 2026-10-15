//! `__transfer_class<T>()`: the compiler intrinsic that evaluates to the transfer class of `T` as
//! an `i64` constant (0 Shared, 1 Moved, 2 Copied), the class argument of `__port_send`
//! (`rt` R7). It is folded in the query layer: a concrete `T` gives the constant directly; a type
//! parameter of the enclosing generic definition is decided per instantiation with
//! [`transfer_class_code_for_substitution`].

use super::super::transfer::{TransferClass, transfer_class, transfer_class_of_type};
use super::super::*;

/// The folded value of one `__transfer_class<T>()` call.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TransferClassIntrinsic {
    /// `T` is concrete (or a type parameter with a Shared, Moved or Copied marker bound).
    Constant(i64),
    /// `T` is the type parameter `name` bounded only by `Transferable`: fold per instantiation.
    PerInstantiation { name: String },
}

/// The intrinsic code of a transfer class; `None` for Never and "some Transferable".
pub fn transfer_class_code(class: &TransferClass) -> Option<i64> {
    match class {
        TransferClass::Shared => Some(0),
        TransferClass::Moved => Some(1),
        TransferClass::Copied => Some(2),
        TransferClass::SomeTransferable | TransferClass::Never(_) => None,
    }
}

pub(in crate::semantic_contract) fn is_transfer_class_intrinsic(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<beskid_analysis::syntax::Type> {
    let call = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    if segment.node.name.node.name != "__transfer_class" || !call.args.is_empty() {
        return None;
    }
    let [argument] = segment.node.type_args.as_slice() else { return None };
    Some(argument.node.clone())
}

/// The folded `__transfer_class<T>()` call at `key`. A Never `T` is unavailable (the `T:
/// Transferable` bound of the corelib callers rules it out).
pub fn transfer_class_intrinsic(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<TransferClassIntrinsic> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    let Some(argument) = with_node(db, syntax, key, |program, index, _node| is_transfer_class_intrinsic(program, index, key))?
    else {
        return Ok(None);
    };
    match transfer_class_of_type(db, key, &argument)? {
        TransferClass::SomeTransferable => match generic_parameter_reference_name(&argument) {
            Some(name) => Ok(Some(TransferClassIntrinsic::PerInstantiation { name: name.to_owned() })),
            None => Err(SemanticError::unavailable_at("transfer_class_intrinsic", key)),
        },
        class => transfer_class_code(&class)
            .map(|code| Some(TransferClassIntrinsic::Constant(code)))
            .ok_or_else(|| SemanticError::unavailable_at("transfer_class_intrinsic", key)),
    }
}

/// The code of a `PerInstantiation` intrinsic under the substitution that binds its type
/// parameter in one specialization.
pub fn transfer_class_code_for_substitution(
    db: &dyn Db,
    key: AstNodeKey,
    substitution: &GenericSubstitution,
) -> Result<i64, SemanticError> {
    let class = transfer_class(db, key, substitution.source_identity())?;
    transfer_class_code(&class).ok_or_else(|| SemanticError::unavailable_at("transfer_class_intrinsic", key))
}
