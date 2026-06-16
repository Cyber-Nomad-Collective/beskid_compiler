//! Callable signatures used during generic inference.

use crate::types::TypeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<TypeId>,
    pub return_type: TypeId,
}
