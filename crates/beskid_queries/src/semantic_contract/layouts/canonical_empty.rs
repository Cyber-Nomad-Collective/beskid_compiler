//! The canonical empty value a destructive read `move x.f` leaves in its field
//! (`language-meta--memory-model--memory-and-references`, "Destructive read of a field").
//!
//! `move_expression_form` names the kind of empty value from the field's type spelling; this
//! module applies the field's exact type, read in the enclosing specialization when the read
//! sits in a generic item, so code generation can build the empty value without a source
//! literal: the `None` variant of the applied `Option<T>` layout, the empty `T[]` with `T`'s
//! element ABI, or a `List<T>` whose `storage` is that empty array and whose `count` is zero.

use super::super::*;
use super::{aggregate_layout_from_definition, enum_layout_for_source_identity, field_read_identity_in_environment};

/// The applied layout of the canonical empty value of one destructive read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CanonicalEmptyValue {
    /// The payload-free `None` variant of the field's applied `Option<T>` layout.
    OptionNone { layout: EnumLayoutFact, variant_index: u32 },
    /// An array of length zero whose element ABI and tracing are those of the field's `T[]`.
    EmptyArray { element: SemanticTypeId, element_managed: ManagedReferenceKind },
    /// A `List<T>` object of the applied `layout` whose `storage` field holds an empty `T[]` and
    /// whose `count` field holds zero; the two field indexes are positions in `layout.fields`.
    EmptyList {
        declaration: AstNodeKey,
        layout: AggregateLayoutFact,
        storage_field: u32,
        count_field: u32,
        element: SemanticTypeId,
        element_managed: ManagedReferenceKind,
    },
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
    let identity = match enclosing.and_then(|enclosing| field_read_identity_in_environment(db, place, &enclosing.substitutions)) {
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
        (CanonicalEmpty::EmptyList, GenericSourceTypeIdentity::Nominal { arguments, .. }) => {
            let [element] = arguments.as_ref() else { return Err(unavailable()) };
            let (element_abi, element_managed) = element_of(element)?;
            let declaration = super::super::contracts::concrete_type_or_enum_declaration(db, place, &identity)
                .ok_or_else(unavailable)?;
            let syntax =
                db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
            let program = syntax.expanded_program(db);
            let index = syntax.syntax_index(db);
            let definition = index
                .node_at(program, declaration.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                .ok_or_else(unavailable)?;
            let [parameter] = definition.generics.as_slice() else { return Err(unavailable()) };
            let shape = match element {
                GenericSourceTypeIdentity::Nominal { .. } => AggregateFieldShape::Nominal(
                    super::super::contracts::concrete_type_or_enum_declaration(db, place, element).ok_or_else(unavailable)?,
                ),
                _ if element_managed == ManagedReferenceKind::GcManaged => AggregateFieldShape::ManagedReference(element_abi),
                _ => AggregateFieldShape::Scalar(element_abi),
            };
            let substitutions = HashMap::from([(parameter.node.name.clone(), shape)]);
            let layout = aggregate_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))?;
            // The list is exactly its backing array and its element count; any other field would
            // need a value the canonical empty does not define.
            let field = |name: &str| layout.fields.iter().position(|(field, _)| field.as_ref() == name);
            let (Some(storage), Some(count)) = (field("storage"), field("count")) else { return Err(unavailable()) };
            if layout.fields.len() != 2
                || !matches!(layout.fields[storage].1, AggregateFieldShape::ManagedReference(_))
                || layout.fields[count].1 != AggregateFieldShape::Scalar(SemanticTypeId::I64)
            {
                return Err(unavailable());
            }
            Ok(Some(CanonicalEmptyValue::EmptyList {
                declaration,
                storage_field: u32::try_from(storage).map_err(|_| unavailable())?,
                count_field: u32::try_from(count).map_err(|_| unavailable())?,
                layout,
                element: element_abi,
                element_managed,
            }))
        }
        _ => Err(unavailable()),
    }
}
