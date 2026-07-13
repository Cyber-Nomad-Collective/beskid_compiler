//! Generated ISLE selection for Beskid's stock CLIF lowering path.

pub use beskid_queries::AstNodeKey;
use cranelift_codegen::ir::InstBuilder;
pub use cranelift_codegen::ir::{AbiParam, Signature, Type, Value};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;

pub const ISLE_INPUTS: &[&str] = &[
    "types.isle",
    "ast.isle",
    "expressions.isle",
    "literals.isle",
    "binary.isle",
    "unary_casts.isle",
    "calls.isle",
    "statements.isle",
    "control_flow.isle",
    "memory.isle",
    "runtime_intrinsics.isle",
    "items.isle",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    IntegerLiteral,
    BooleanLiteral,
    Grouped,
    UnaryNeg,
    UnaryNot,
    BinaryAdd,
    Unsupported,
}

#[allow(
    unused_imports,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::len_without_is_empty,
    clippy::match_ref_pats
)]
pub mod generated {
    use super::{AstNodeKey, NodeKind, Value};

    include!(concat!(env!("OUT_DIR"), "/beskid_lower.rs"));
}

include!(concat!(env!("OUT_DIR"), "/beskid_isle_metadata.rs"));

/// Scalar facts consumed by leaf ISLE rules.
///
/// The frontend adapter implements this trait with generation-checked Salsa queries. It is a
/// compile-time boundary while those queries are integrated, not a second semantic model.
pub trait NodeFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind>;
    fn child(&self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
        None
    }
    fn integer_literal(&self, key: AstNodeKey) -> Option<i64>;
    fn boolean_literal(&self, _key: AstNodeKey) -> Option<bool> {
        None
    }
    fn scalar_type(&self, key: AstNodeKey) -> Option<Type>;
}

/// Thin host for generated ISLE selection and stock CLIF instruction construction.
pub struct IsleContext<'builder, 'function, 'facts> {
    builder: &'builder mut FunctionBuilder<'function>,
    facts: &'facts dyn NodeFacts,
}

impl<'builder, 'function, 'facts> IsleContext<'builder, 'function, 'facts> {
    pub fn new(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
    ) -> Self {
        Self { builder, facts }
    }
}

impl generated::Context for IsleContext<'_, '_, '_> {
    fn node_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts.node_kind(key)
    }

    fn child_at(&mut self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        self.facts.child(key, index)
    }

    fn emit_integer(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = self.facts.integer_literal(key)?;
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    fn emit_boolean(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = i64::from(self.facts.boolean_literal(key)?);
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    fn clif_iadd(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().iadd(left, right)
    }

    fn clif_ineg(&mut self, value: Value) -> Value {
        self.builder.ins().ineg(value)
    }

    fn clif_bnot(&mut self, value: Value) -> Value {
        self.builder.ins().bnot(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoweringError {
    key: AstNodeKey,
}

impl LoweringError {
    pub fn key(self) -> AstNodeKey {
        self.key
    }
}

pub fn lower_expression(
    context: &mut IsleContext<'_, '_, '_>,
    key: AstNodeKey,
) -> Result<Value, LoweringError> {
    generated::constructor_lower_expression(context, key).ok_or(LoweringError { key })
}

/// ISA-owned signature construction shared by every generated function kind.
pub struct FunctionEmitter<'isa> {
    isa: &'isa dyn TargetIsa,
}

impl<'isa> FunctionEmitter<'isa> {
    pub fn new(isa: &'isa dyn TargetIsa) -> Self {
        Self { isa }
    }

    pub fn pointer_type(&self) -> Type {
        self.isa.pointer_type()
    }

    pub fn signature(
        &self,
        parameters: impl IntoIterator<Item = Type>,
        returns: impl IntoIterator<Item = Type>,
    ) -> Signature {
        let mut signature = Signature::new(self.isa.default_call_conv());
        signature
            .params
            .extend(parameters.into_iter().map(AbiParam::new));
        signature
            .returns
            .extend(returns.into_iter().map(AbiParam::new));
        signature
    }
}
