//! The canonical empty value a destructive read `move x.f` leaves in its field
//! (`language-meta--memory-model--memory-and-references`, "Destructive read of a field").
//!
//! `move_expression_form` names the kind of empty value from the field's type spelling; this
//! module applies the field's exact type, read in the enclosing specialization when the read
//! sits in a generic item, so code generation can build the empty value without a source
//! literal: the `None` variant of the applied `Option<T>` layout or the empty `T[]` with `T`'s
//! element ABI.

use super::super::*;
use super::{enum_layout_for_source_identity, field_read_identity_in_environment};

/// The applied layout of the canonical empty value of one destructive read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CanonicalEmptyValue {
    /// The payload-free `None` variant of the field's applied `Option<T>` layout.
    OptionNone { layout: EnumLayoutFact, variant_index: u32 },
    /// An array of length zero whose element ABI and tracing are those of the field's `T[]`.
    EmptyArray { element: SemanticTypeId, element_managed: ManagedReferenceKind },
}

/// The canonical empty value of the destructive read `move x.f` at `key`, with the field type
/// read in `enclosing` when the read sits in the body of a specialized generic item. A `move`
/// that consumes a name (and every other node) has no fact; a field type that the applied
/// identity does not prove is an error, never a guessed layout.
pub fn canonical_empty_value(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<CanonicalEmptyValue> {
    let Some(MoveExpressionForm::DestructiveRead { place, empty, .. }) = crate::move_expression_form(db, key)? else {
        return Ok(None);
    };
    let identity = match enclosing.and_then(|enclosing| field_read_identity_in_environment(db, place, enclosing)) {
        Some(identity) => identity?,
        None => generic_source_expression_identity(db, place)?,
    };
    let unavailable = || SemanticError::unavailable_at("canonical_empty_value", key);
    let element_of = |element: &GenericSourceTypeIdentity| {
        let abi = element.abi_type();
        (!matches!(abi, SemanticTypeId::UNIT | SemanticTypeId::NEVER))
            .then(|| (abi, element.managed_reference_kind()))
            .ok_or_else(unavailable)
    };
    match (empty, &identity) {
        (CanonicalEmpty::EmptyArray, GenericSourceTypeIdentity::Array(element)) => {
            let (element, element_managed) = element_of(element)?;
            Ok(Some(CanonicalEmptyValue::EmptyArray { element, element_managed }))
        }
        (CanonicalEmpty::OptionNone, GenericSourceTypeIdentity::Nominal { .. }) => {
            let (_, layout) = enum_layout_for_source_identity(db, place, &identity)?;
            let variant_index = layout
                .variants
                .iter()
                .position(|variant| variant.name.as_ref() == "None" && variant.fields.is_empty())
                .and_then(|index| u32::try_from(index).ok())
                .ok_or_else(unavailable)?;
            Ok(Some(CanonicalEmptyValue::OptionNone { layout, variant_index }))
        }
        _ => Err(unavailable()),
    }
}
