//! The expected type of one expression: the type its typed parent context checks it against.
//!
//! This is the checking direction of bidirectional typing (Pierce and Turner, local type
//! inference; rustc's `Expectation`; Swift and TypeScript contextual typing). Synthesis facts
//! (`abi_type`, `node_type`, source identities) describe what an expression *is*; the expected
//! type describes what its position *requires*, and it exists only where a declaration or a
//! sibling already proves that requirement. No expected type is ever guessed from the expression
//! itself.

use std::sync::Arc;

use super::generics::GenericSourceTypeIdentity;
use super::ids::AstNodeKey;
use super::types::{ManagedReferenceKind, SemanticTypeId};

/// The typed parent context that supplies an expected type. The variants name the innermost
/// context; a context that forwards its own expected type (a match arm, an array element, a
/// block result) still reports itself so a consumer can see how the type reached the node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExpectedTypeContext {
    /// The initializer of an explicitly annotated `let` (`u8 value = ...`).
    DeclaredLocal { statement: AstNodeKey },
    /// The value of an assignment; the target's declared or projected type is the expectation.
    Assignment { assignment: AstNodeKey },
    /// One argument of a call, by zero-based position in the written argument list.
    CallArgument { call: AstNodeKey, position: u32 },
    /// One payload argument of an enum constructor, by zero-based position.
    EnumPayload { constructor: AstNodeKey, position: u32 },
    /// The value of one named field in a struct literal.
    StructField { literal: AstNodeKey, field: Arc<str> },
    /// The value of a `return` in a function or method with a declared return type.
    Return { item: AstNodeKey },
    /// One element of an array literal whose own expected type is an array.
    ArrayElement { array: AstNodeKey },
    /// The body of a match arm; the match expression's expected type is forwarded.
    MatchArm { expression: AstNodeKey },
    /// The value-producing last statement of a block expression; the block's expected type is
    /// forwarded.
    BlockResult { block: AstNodeKey },
    /// One operand of a binary expression whose peer operand proves the representation.
    BinaryOperand { expression: AstNodeKey, peer: AstNodeKey },
    /// An expression statement discards its value: the expectation is `unit`.
    ExpressionStatement { statement: AstNodeKey },
}

/// The source type one expression is checked against, with the context that supplies it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExpectedType {
    pub context: ExpectedTypeContext,
    pub(crate) identity: GenericSourceTypeIdentity,
}

impl ExpectedType {
    pub(crate) fn new(context: ExpectedTypeContext, identity: GenericSourceTypeIdentity) -> Self {
        Self { context, identity }
    }

    /// The target ABI representation of the expected type.
    pub fn abi_type(&self) -> SemanticTypeId {
        self.identity.abi_type()
    }

    /// Whether values of the expected type are traced by the garbage collector.
    pub fn managed_reference_kind(&self) -> ManagedReferenceKind {
        self.identity.managed_reference_kind()
    }

    /// The element ABI when the expected type is an array (`T[]`); `None` otherwise.
    pub fn array_element_abi_type(&self) -> Option<SemanticTypeId> {
        self.array_element_identity().map(GenericSourceTypeIdentity::abi_type)
    }

    /// The source spelling of the expected type for diagnostics.
    pub fn display_name(&self) -> String {
        self.identity.display_name()
    }

    pub(crate) fn identity(&self) -> &GenericSourceTypeIdentity {
        &self.identity
    }

    pub(crate) fn array_element_identity(&self) -> Option<&GenericSourceTypeIdentity> {
        match &self.identity {
            GenericSourceTypeIdentity::Array(element) => Some(element),
            GenericSourceTypeIdentity::Abi(_)
            | GenericSourceTypeIdentity::Nominal { .. }
            | GenericSourceTypeIdentity::Function { .. } => None,
        }
    }
}
