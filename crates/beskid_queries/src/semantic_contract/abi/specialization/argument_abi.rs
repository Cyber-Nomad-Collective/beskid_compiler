//! Contextual integer-literal ABI fit checks shared by the expected-type consumers.

use super::super::super::*;
use super::*;

/// Whether a source expression is a bare integer literal without an ABI suffix.
///
/// This follows only singleton syntax wrappers, so compound arithmetic, casts, calls, and arrays
/// never inherit an ABI representation from a surrounding call.
pub(in crate::semantic_contract) fn unsuffixed_integer_literal(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<bool, SemanticError> {
    Ok(integer_literal_text(db, key)?.is_some())
}

/// The elements of an array literal whose every element is a bare integer literal (`[7, 8]`),
/// through singleton syntax wrappers; `None` for any other expression and for `[]`.
///
/// Such a literal has no source identity of its own: its elements take their width from the
/// position they are written into. A generic call therefore binds `T` of a `T[]` parameter from
/// it exactly as it binds `T` of a `T` parameter from a bare integer: provisionally at the
/// language default `i32`, replaceable by a later exact argument that every element fits.
pub(in crate::semantic_contract) fn unsuffixed_integer_array_elements(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<Vec<AstNodeKey>>, SemanticError> {
    let mut key = key;
    loop {
        match node_kind(db, key)? {
            Some(IndexedNodeKind::ArrayLiteralExpression) => break,
            Some(IndexedNodeKind::Expression | IndexedNodeKind::GroupedExpression) => {
                let Some(children) = child_nodes(db, key)? else { return Ok(None) };
                let [child] = children.as_ref() else { return Ok(None) };
                key = *child;
            }
            _ => return Ok(None),
        }
    }
    let Some(elements) = child_nodes(db, key)? else { return Ok(None) };
    if elements.is_empty() {
        return Ok(None);
    }
    for element in elements.iter() {
        if !unsuffixed_integer_literal(db, *element)? {
            return Ok(None);
        }
    }
    Ok(Some(elements.to_vec()))
}

/// Whether every bare integer literal of `literals` fits `expected`.
pub(in crate::semantic_contract) fn integer_literals_fit_abi(
    db: &dyn Db,
    literals: &[AstNodeKey],
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    for literal in literals {
        if !integer_literal_fits_abi(db, *literal, expected)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Prove that one bare integer literal fits the ABI representation selected elsewhere in its
/// generic call. Explicitly suffixed literals never reach this helper.
pub(in crate::semantic_contract) fn integer_literal_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    let Some(text) = integer_literal_text(db, key)? else {
        return Ok(false);
    };
    if let Some(magnitude) = text.strip_prefix('-').and_then(integer_literal_u64) {
        return Ok(match expected {
            SemanticTypeId::I8 => magnitude <= (i8::MAX as u64) + 1,
            SemanticTypeId::I16 => magnitude <= (i16::MAX as u64) + 1,
            SemanticTypeId::I32 => magnitude <= (i32::MAX as u64) + 1,
            SemanticTypeId::I64 => magnitude <= (i64::MAX as u64) + 1,
            SemanticTypeId::U16
            | SemanticTypeId::U64
            | SemanticTypeId::U32
            | SemanticTypeId::U8
            | SemanticTypeId::WORD => false,
            _ => false,
        });
    }
    let value = integer_literal_u64(&text);
    Ok(match expected {
        SemanticTypeId::I8 => value.is_some_and(|v| i8::try_from(v).is_ok()),
        SemanticTypeId::I16 => value.is_some_and(|v| i16::try_from(v).is_ok()),
        SemanticTypeId::U16 => value.is_some_and(|v| u16::try_from(v).is_ok()),
        SemanticTypeId::U64 => value.is_some(),
        SemanticTypeId::I32 => value.is_some_and(|value| i32::try_from(value).is_ok()),
        SemanticTypeId::I64 => value.is_some_and(|value| i64::try_from(value).is_ok()),
        SemanticTypeId::U32 => value.is_some_and(|value| u32::try_from(value).is_ok()),
        SemanticTypeId::U8 => value.is_some_and(|value| u8::try_from(value).is_ok()),
        SemanticTypeId::WORD => value.is_some(),
        _ => false,
    })
}

/// Whether a bare integer literal or integer constant fits one expected primitive integer ABI.
pub(in crate::semantic_contract) fn contextual_integer_operand_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    if integer_literal_text(db, key)?.is_some() {
        return integer_literal_fits_abi(db, key, expected);
    }
    // A constant use behaves as its declared literal: it fits exactly where that literal fits,
    // judged on the literal's text so a word-sized bit pattern is never read as a negative i64.
    let Some(text) = contextual_constant_literal(db, key)? else {
        return Ok(false);
    };
    Ok(integer_primitive_for_semantic(expected)
        .is_some_and(|primitive| beskid_analysis::syntax::integer_literal_fits_primitive(&text, primitive)))
}

pub(in crate::semantic_contract) fn integer_literal_text(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<Arc<str>>, SemanticError> {
    let Some(literal) = literal_fact(db, key)? else {
        let Some(children) = child_nodes(db, key)? else {
            return Ok(None);
        };
        if node_kind(db, key)? == Some(IndexedNodeKind::UnaryExpression)
            && operator_fact(db, key)? == Some(OperatorFact::Neg)
        {
            let [_, operand] = children.as_ref() else {
                return Ok(None);
            };
            return Ok(integer_literal_text(db, *operand)?.map(|text| Arc::from(format!("-{text}"))));
        }
        let [child] = children.as_ref() else {
            return Ok(None);
        };
        // A nullary lambda (`() => 20`) has one child, its body; the lambda is not a literal.
        if node_kind(db, key)? == Some(IndexedNodeKind::LambdaExpression) {
            return Ok(None);
        }
        return integer_literal_text(db, *child);
    };
    match literal {
        LiteralFact::Integer(text) if !integer_has_explicit_abi_suffix(&text) => Ok(Some(text)),
        _ => Ok(None),
    }
}

pub(in crate::semantic_contract) fn contextual_constant_integer(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<i64>, SemanticError> {
    if let Some(value) = constant_integer(db, key)? {
        return Ok(Some(value));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(None);
    };
    let [child] = children.as_ref() else {
        return Ok(None);
    };
    contextual_constant_integer(db, *child)
}

/// The declared literal of a module constant use, through singleton wrappers.
pub(in crate::semantic_contract) fn contextual_constant_literal(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<Arc<str>>, SemanticError> {
    if let Some(text) = constant_integer_literal(db, key)? {
        return Ok(Some(text));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(None);
    };
    let [child] = children.as_ref() else {
        return Ok(None);
    };
    contextual_constant_literal(db, *child)
}

pub(in crate::semantic_contract) fn integer_has_explicit_abi_suffix(text: &str) -> bool {
    matches!(
        text.rsplit_once('_').map(|(_, suffix)| suffix),
        Some("i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "word")
    )
}

/// Parse a source integer's magnitude while preserving a hexadecimal word-sized bit pattern.
///
/// The caller is responsible for ABI suffix handling. Negative values are deliberately excluded:
/// this helper is used only for selecting and checking unsigned ABI representations.
pub(in crate::semantic_contract) fn integer_literal_u64(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(digits) => u64::from_str_radix(&digits.replace('_', ""), 16).ok(),
        None => text.replace('_', "").parse::<u64>().ok(),
    }
}
