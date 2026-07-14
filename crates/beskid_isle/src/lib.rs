//! Generated ISLE selection for Beskid's stock CLIF lowering path.
//!
//! Raw constructors are an implementation detail of [`FunctionEmitter`]:
//!
//! ```compile_fail
//! use beskid_isle::generated;
//! ```

pub use beskid_queries::AstNodeKey;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::types;
pub use cranelift_codegen::ir::{
    AbiParam, FuncRef, Function, Signature, Type, UserFuncName, Value,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::FunctionBuilder;
use cranelift_frontend::FunctionBuilderContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Program,
    FunctionDefinition,
    ExpressionStatement,
    ReturnStatement,
    IfStatement,
    LiteralExpression,
    GroupedExpression,
    UnaryExpression,
    BinaryExpression,
    CallExpression,
    PathExpression,
    BlockExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StatementCursor {
    block: AstNodeKey,
    index: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorKind {
    More,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralKind {
    Integer,
    Float,
    String,
    Char,
    Boolean,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorFact {
    Or,
    And,
    IdentityEq,
    IdentityNotEq,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Direct,
    RuntimeIntrinsic,
}

pub type Unit = ();

#[allow(
    dead_code,
    unused_imports,
    unreachable_patterns,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::len_without_is_empty,
    clippy::let_unit_value,
    clippy::match_ref_pats
)]
mod generated {
    use super::{
        AstNodeKey, CallKind, CursorKind, LiteralKind, NodeKind, OperatorFact, StatementCursor,
        Unit, Value,
    };

    include!(concat!(env!("OUT_DIR"), "/beskid_lower.rs"));
}

include!(concat!(env!("OUT_DIR"), "/beskid_isle_metadata.rs"));

/// Scalar facts consumed by leaf ISLE rules.
///
/// The frontend adapter implements this trait with generation-checked Salsa queries. It is a
/// compile-time boundary while those queries are integrated, not a second semantic model.
pub trait NodeFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind>;
    fn literal_kind(&self, _key: AstNodeKey) -> Option<LiteralKind> {
        None
    }
    fn operator_fact(&self, _key: AstNodeKey) -> Option<OperatorFact> {
        None
    }
    fn call_kind(&self, _key: AstNodeKey) -> Option<CallKind> {
        None
    }
    fn child(&self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
        None
    }
    fn statement_count(&self, _key: AstNodeKey) -> Option<u8> {
        None
    }
    fn integer_literal(&self, key: AstNodeKey) -> Option<i64>;
    fn boolean_literal(&self, _key: AstNodeKey) -> Option<bool> {
        None
    }
    fn float_literal(&self, _key: AstNodeKey) -> Option<f64> {
        None
    }
    fn char_literal(&self, _key: AstNodeKey) -> Option<char> {
        None
    }
    fn string_literal(&self, _key: AstNodeKey) -> Option<&str> {
        None
    }
    fn scalar_type(&self, key: AstNodeKey) -> Option<Type>;
    fn call_target(&self, _key: AstNodeKey) -> Option<FuncRef> {
        None
    }
    fn call_arguments(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn local_value(&self, _key: AstNodeKey) -> Option<Value> {
        None
    }
}

/// Artifact-owned string materialization invoked only after generated ISLE selection.
pub trait StringInterner {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        key: AstNodeKey,
        text: &str,
    ) -> Option<Value>;
}

/// Thin host for generated ISLE selection and stock CLIF instruction construction.
pub struct IsleContext<'builder, 'function, 'facts, 'interner> {
    builder: &'builder mut FunctionBuilder<'function>,
    facts: &'facts dyn NodeFacts,
    string_interner: Option<&'interner mut dyn StringInterner>,
}

impl<'builder, 'function, 'facts, 'interner> IsleContext<'builder, 'function, 'facts, 'interner> {
    pub fn new(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner: None,
        }
    }

    pub fn new_with_string_interner(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        string_interner: &'interner mut dyn StringInterner,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner: Some(string_interner),
        }
    }

    fn short_circuit(&mut self, key: AstNodeKey, branch_on_true: bool) -> Option<Value> {
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = generated::constructor_lower_expression(self, left_key)?;
        let value_type = self.builder.func.dfg.value_type(left);
        let right_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, value_type);

        if branch_on_true {
            self.builder
                .ins()
                .brif(left, merge_block, &[left.into()], right_block, &[]);
        } else {
            self.builder
                .ins()
                .brif(left, right_block, &[], merge_block, &[left.into()]);
        }

        self.builder.switch_to_block(right_block);
        self.builder.seal_block(right_block);
        let right = generated::constructor_lower_expression(self, right_key)?;
        self.builder.ins().jump(merge_block, &[right.into()]);
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        self.builder.block_params(merge_block).first().copied()
    }

    fn direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let function = self.facts.call_target(key)?;
        let argument_keys = self.facts.call_arguments(key)?;
        let mut arguments = Vec::with_capacity(argument_keys.len());
        for argument in argument_keys {
            arguments.push(generated::constructor_lower_expression(self, argument)?);
        }
        let call = self.builder.ins().call(function, &arguments);
        self.builder.inst_results(call).first().copied()
    }
}

impl generated::Context for IsleContext<'_, '_, '_, '_> {
    fn node_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts.node_kind(key)
    }

    fn literal_kind(&mut self, key: AstNodeKey) -> Option<LiteralKind> {
        self.facts.literal_kind(key)
    }

    fn operator_fact(&mut self, key: AstNodeKey) -> Option<OperatorFact> {
        self.facts.operator_fact(key)
    }

    fn call_kind(&mut self, key: AstNodeKey) -> Option<CallKind> {
        self.facts.call_kind(key)
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

    fn emit_float(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = self.facts.float_literal(key)?;
        match self.facts.scalar_type(key)? {
            types::F32 => Some(
                self.builder
                    .ins()
                    .f32const(Ieee32::with_float(immediate as f32)),
            ),
            types::F64 => Some(self.builder.ins().f64const(Ieee64::with_float(immediate))),
            _ => None,
        }
    }

    fn emit_char(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = i64::from(u32::from(self.facts.char_literal(key)?));
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    fn emit_string(&mut self, key: AstNodeKey) -> Option<Value> {
        let text = self.facts.string_literal(key)?;
        self.string_interner
            .as_deref_mut()?
            .intern(self.builder, key, text)
    }

    fn clif_iadd(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().iadd(left, right)
    }

    fn clif_isub(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().isub(left, right)
    }

    fn clif_imul(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().imul(left, right)
    }

    fn clif_sdiv(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().sdiv(left, right)
    }

    fn clif_srem(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().srem(left, right)
    }

    fn clif_eq(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::Equal, left, right)
    }

    fn clif_ne(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::NotEqual, left, right)
    }

    fn clif_slt(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::SignedLessThan, left, right)
    }

    fn clif_sle(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::SignedLessThanOrEqual, left, right)
    }

    fn clif_sgt(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::SignedGreaterThan, left, right)
    }

    fn clif_sge(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, left, right)
    }

    fn clif_short_circuit_or(&mut self, key: AstNodeKey) -> Option<Value> {
        self.short_circuit(key, true)
    }

    fn clif_short_circuit_and(&mut self, key: AstNodeKey) -> Option<Value> {
        self.short_circuit(key, false)
    }

    fn clif_ineg(&mut self, value: Value) -> Value {
        self.builder.ins().ineg(value)
    }

    fn clif_bnot(&mut self, value: Value) -> Value {
        self.builder.ins().icmp_imm(IntCC::Equal, value, 0)
    }

    fn emit_direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
        self.direct_call(key)
    }

    fn emit_runtime_intrinsic(&mut self, key: AstNodeKey) -> Option<Value> {
        self.direct_call(key)
    }

    fn discard_value(&mut self, _value: Value) {}

    fn emit_return(&mut self, key: AstNodeKey) -> Option<()> {
        if let Some(value_key) = self.facts.child(key, 0) {
            let value = generated::constructor_lower_expression(self, value_key)?;
            self.builder.ins().return_(&[value]);
        } else {
            self.builder.ins().return_(&[]);
        }
        Some(())
    }

    fn emit_if_else(&mut self, key: AstNodeKey) -> Option<()> {
        let condition_key = self.facts.child(key, 0)?;
        let then_key = self.facts.child(key, 1)?;
        let else_key = self.facts.child(key, 2)?;
        let condition = generated::constructor_lower_expression(self, condition_key)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(condition, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        generated::constructor_lower_statement(self, then_key)?;

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        generated::constructor_lower_statement(self, else_key)?;
        Some(())
    }

    fn statement_cursor(&mut self, key: AstNodeKey) -> Option<StatementCursor> {
        self.facts.statement_count(key)?;
        Some(StatementCursor {
            block: key,
            index: 0,
        })
    }

    fn cursor_kind(&mut self, cursor: StatementCursor) -> Option<CursorKind> {
        let count = self.facts.statement_count(cursor.block)?;
        Some(if cursor.index < count {
            CursorKind::More
        } else {
            CursorKind::End
        })
    }

    fn cursor_head(&mut self, cursor: StatementCursor) -> Option<AstNodeKey> {
        self.facts.child(cursor.block, cursor.index)
    }

    fn cursor_tail(&mut self, cursor: StatementCursor) -> StatementCursor {
        StatementCursor {
            block: cursor.block,
            index: cursor.index.saturating_add(1),
        }
    }

    fn finish_statements(&mut self) {}

    fn sequence_statements(&mut self, _head: (), _tail: ()) {}

    fn emit_local_read(&mut self, key: AstNodeKey) -> Option<Value> {
        self.facts.local_value(key)
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
    context: &mut IsleContext<'_, '_, '_, '_>,
    key: AstNodeKey,
) -> Result<Value, LoweringError> {
    generated::constructor_lower_expression(context, key).ok_or(LoweringError { key })
}

pub fn lower_statement(
    context: &mut IsleContext<'_, '_, '_, '_>,
    key: AstNodeKey,
) -> Result<(), LoweringError> {
    generated::constructor_lower_statement(context, key).ok_or(LoweringError { key })
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

    pub fn emit_expression(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, None)
    }

    pub fn emit_expression_with_string_interner(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: &mut dyn StringInterner,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, Some(string_interner))
    }

    pub fn emit_statement(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            lower_statement(&mut IsleContext::new(&mut builder, facts), body)
                .map_err(FunctionEmissionError::Lowering)?;
            let terminated = builder
                .func
                .layout
                .last_inst(entry)
                .is_some_and(|inst| builder.func.dfg.insts[inst].opcode().is_terminator());
            if !terminated {
                return Err(FunctionEmissionError::Verification(
                    "generated statement body did not terminate its entry block".to_owned(),
                ));
            }
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::Verification(error.to_string()))?;
        Ok(function)
    }

    fn emit_expression_inner(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: Option<&mut dyn StringInterner>,
    ) -> Result<Function, FunctionEmissionError> {
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let value = match string_interner {
                Some(interner) => lower_expression(
                    &mut IsleContext::new_with_string_interner(&mut builder, facts, interner),
                    body,
                ),
                None => lower_expression(&mut IsleContext::new(&mut builder, facts), body),
            }
            .map_err(FunctionEmissionError::Lowering)?;
            builder.ins().return_(&[value]);
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::Verification(error.to_string()))?;
        Ok(function)
    }
}

#[derive(Debug)]
pub enum FunctionEmissionError {
    Lowering(LoweringError),
    Verification(String),
}
