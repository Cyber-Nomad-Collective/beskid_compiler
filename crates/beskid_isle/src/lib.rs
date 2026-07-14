//! Generated ISLE selection for Beskid's stock CLIF lowering path.
//!
//! Raw constructors are an implementation detail of [`FunctionEmitter`]:
//!
//! ```compile_fail
//! use beskid_isle::generated;
//! ```

use std::collections::HashMap;

pub use beskid_queries::AstNodeKey;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::types;
pub use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, MemFlags, Signature, StackSlotData, StackSlotKind,
    TrapCode, Type, UserFuncName, Value,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::FunctionBuilder;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_frontend::Variable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Program,
    FunctionDefinition,
    ExpressionStatement,
    ReturnStatement,
    LetStatement,
    IfStatement,
    WhileStatement,
    BreakStatement,
    ContinueStatement,
    LiteralExpression,
    GroupedExpression,
    UnaryExpression,
    BinaryExpression,
    AssignExpression,
    CallExpression,
    PathExpression,
    IndexExpression,
    ArrayLiteralExpression,
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

#[derive(Clone, Copy)]
struct LoopTargets {
    continue_block: Block,
    break_block: Block,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectCallee(u32);

impl DirectCallee {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallImportError {
    UnknownCallee,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayLayout {
    element_type: Type,
    stride: u32,
    length: u32,
    align_shift: u8,
}

impl ArrayLayout {
    pub const fn new(element_type: Type, stride: u32, length: u32, align_shift: u8) -> Self {
        Self {
            element_type,
            stride,
            length,
            align_shift,
        }
    }

    fn byte_size(self) -> Option<u32> {
        self.stride.checked_mul(self.length)
    }

    fn is_valid(self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        self.element_type.bytes() > 0
            && self.stride >= self.element_type.bytes()
            && self.stride.is_multiple_of(alignment)
            && self.byte_size().is_some_and(|size| size <= i32::MAX as u32)
    }
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
    fn let_initializer(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
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
    fn direct_callee(&self, _key: AstNodeKey) -> Option<DirectCallee> {
        None
    }
    fn call_signature(&self, _key: AstNodeKey) -> Option<Signature> {
        None
    }
    fn call_arguments(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn array_elements(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn array_layout(&self, _key: AstNodeKey) -> Option<ArrayLayout> {
        None
    }
    fn local_slot(&self, _key: AstNodeKey) -> Option<u32> {
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

/// Caller-local import of a semantic callee after generated ISLE selection.
pub trait CallImporter {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, CallImportError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoweringErrorKind {
    MissingRuleOrFact,
    UnknownCallee(DirectCallee),
    InvalidArrayLayout,
}

/// Thin host for generated ISLE selection and stock CLIF instruction construction.
pub struct IsleContext<'builder, 'function, 'facts, 'interner> {
    builder: &'builder mut FunctionBuilder<'function>,
    facts: &'facts dyn NodeFacts,
    string_interner: Option<&'interner mut dyn StringInterner>,
    call_importer: Option<&'interner mut dyn CallImporter>,
    loop_stack: Vec<LoopTargets>,
    locals: HashMap<u32, (Variable, Type)>,
    pending_error: Option<LoweringError>,
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
            call_importer: None,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            pending_error: None,
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
            call_importer: None,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            pending_error: None,
        }
    }

    pub fn new_with_call_importer(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        call_importer: &'interner mut dyn CallImporter,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner: None,
            call_importer: Some(call_importer),
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            pending_error: None,
        }
    }

    fn new_with_services(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        string_interner: Option<&'interner mut dyn StringInterner>,
        call_importer: Option<&'interner mut dyn CallImporter>,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner,
            call_importer,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            pending_error: None,
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
        let callee = self.facts.direct_callee(key)?;
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.returns.len() != 1 || signature.returns[0].value_type != result_type {
            return None;
        }
        let function =
            match self
                .call_importer
                .as_deref_mut()?
                .import(self.builder, callee, &signature)
            {
                Ok(function) => function,
                Err(CallImportError::UnknownCallee) => {
                    self.pending_error = Some(LoweringError {
                        key,
                        kind: LoweringErrorKind::UnknownCallee(callee),
                    });
                    return None;
                }
            };
        let argument_keys = self.facts.call_arguments(key)?;
        if argument_keys.len() != signature.params.len() {
            return None;
        }
        let mut arguments = Vec::with_capacity(argument_keys.len());
        for (argument, parameter) in argument_keys.into_iter().zip(&signature.params) {
            let value = generated::constructor_lower_expression(self, argument)?;
            if self.builder.func.dfg.value_type(value) != parameter.value_type {
                return None;
            }
            arguments.push(value);
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

    fn emit_local_let(&mut self, key: AstNodeKey) -> Option<()> {
        let slot = self.facts.local_slot(key)?;
        if self.locals.contains_key(&slot) {
            return None;
        }
        let initializer = self.facts.let_initializer(key)?;
        let value = generated::constructor_lower_expression(self, initializer)?;
        let value_type = self.facts.scalar_type(key)?;
        if self.builder.func.dfg.value_type(value) != value_type {
            return None;
        }
        let variable = self.builder.declare_var(value_type);
        self.builder.def_var(variable, value);
        self.locals.insert(slot, (variable, value_type));
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

    fn emit_while(&mut self, key: AstNodeKey) -> Option<()> {
        let condition_key = self.facts.child(key, 0)?;
        let body_key = self.facts.child(key, 1)?;
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let condition = generated::constructor_lower_expression(self, condition_key)?;
        self.builder.ins().brif(condition, body, &[], exit, &[]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        self.loop_stack.push(LoopTargets {
            continue_block: header,
            break_block: exit,
        });
        let lowered = generated::constructor_lower_statement(self, body_key);
        self.loop_stack.pop();
        lowered?;
        if !block_is_terminated(self.builder, body) {
            self.builder.ins().jump(header, &[]);
        }

        self.builder.seal_block(header);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }

    fn emit_break(&mut self, _key: AstNodeKey) -> Option<()> {
        let target = self.loop_stack.last()?.break_block;
        self.builder.ins().jump(target, &[]);
        Some(())
    }

    fn emit_continue(&mut self, _key: AstNodeKey) -> Option<()> {
        let target = self.loop_stack.last()?.continue_block;
        self.builder.ins().jump(target, &[]);
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
        let slot = self.facts.local_slot(key)?;
        let (variable, _) = self.locals.get(&slot).copied()?;
        Some(self.builder.use_var(variable))
    }

    fn emit_local_assign(&mut self, key: AstNodeKey) -> Option<Value> {
        let target = self.facts.child(key, 0)?;
        let value_key = self.facts.child(key, 1)?;
        let slot = self.facts.local_slot(target)?;
        let (variable, expected_type) = self.locals.get(&slot).copied()?;
        let value = generated::constructor_lower_expression(self, value_key)?;
        if self.builder.func.dfg.value_type(value) != expected_type {
            return None;
        }
        self.builder.def_var(variable, value);
        Some(value)
    }

    fn emit_array_literal(&mut self, key: AstNodeKey) -> Option<Value> {
        let elements = self.facts.array_elements(key)?;
        let layout = self.facts.array_layout(key)?;
        if !layout.is_valid() || usize::try_from(layout.length).ok()? != elements.len() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidArrayLayout,
            });
            return None;
        }
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.byte_size()?.max(1),
            layout.align_shift,
        ));
        for (index, element) in elements.into_iter().enumerate() {
            let value = generated::constructor_lower_expression(self, element)?;
            if self.builder.func.dfg.value_type(value) != layout.element_type {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidArrayLayout,
                });
                return None;
            }
            let offset = u32::try_from(index)
                .ok()?
                .checked_mul(layout.stride)
                .and_then(|offset| i32::try_from(offset).ok())?;
            self.builder.ins().stack_store(value, slot, offset);
        }
        let pointer_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().stack_addr(pointer_type, slot, 0))
    }

    fn emit_index_read(&mut self, key: AstNodeKey) -> Option<Value> {
        let layout = self.facts.array_layout(key)?;
        if !layout.is_valid() || self.facts.scalar_type(key)? != layout.element_type {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidArrayLayout,
            });
            return None;
        }
        let base_key = self.facts.child(key, 0)?;
        let index_key = self.facts.child(key, 1)?;
        let base = generated::constructor_lower_expression(self, base_key)?;
        let index = generated::constructor_lower_expression(self, index_key)?;
        let index_type = self.builder.func.dfg.value_type(index);
        let pointer_type = self.builder.func.dfg.value_type(base);
        if !index_type.is_int() || !pointer_type.is_int() {
            return None;
        }
        let out_of_bounds = self.builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            index,
            i64::from(layout.length),
        );
        self.builder
            .ins()
            .trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
        let pointer_index = if index_type.bits() < pointer_type.bits() {
            self.builder.ins().uextend(pointer_type, index)
        } else if index_type.bits() > pointer_type.bits() {
            self.builder.ins().ireduce(pointer_type, index)
        } else {
            index
        };
        let offset = if layout.stride == 1 {
            pointer_index
        } else {
            self.builder
                .ins()
                .imul_imm(pointer_index, i64::from(layout.stride))
        };
        let address = self.builder.ins().iadd(base, offset);
        Some(
            self.builder
                .ins()
                .load(layout.element_type, MemFlags::new(), address, 0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoweringError {
    key: AstNodeKey,
    kind: LoweringErrorKind,
}

impl LoweringError {
    pub fn key(self) -> AstNodeKey {
        self.key
    }

    pub fn kind(self) -> LoweringErrorKind {
        self.kind
    }
}

pub fn lower_expression(
    context: &mut IsleContext<'_, '_, '_, '_>,
    key: AstNodeKey,
) -> Result<Value, LoweringError> {
    generated::constructor_lower_expression(context, key).ok_or_else(|| {
        context.pending_error.take().unwrap_or(LoweringError {
            key,
            kind: LoweringErrorKind::MissingRuleOrFact,
        })
    })
}

pub fn lower_statement(
    context: &mut IsleContext<'_, '_, '_, '_>,
    key: AstNodeKey,
) -> Result<(), LoweringError> {
    generated::constructor_lower_statement(context, key).ok_or_else(|| {
        context.pending_error.take().unwrap_or(LoweringError {
            key,
            kind: LoweringErrorKind::MissingRuleOrFact,
        })
    })
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
        self.emit_expression_inner(name, signature, facts, body, None, None)
    }

    pub fn emit_expression_with_string_interner(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: &mut dyn StringInterner,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, Some(string_interner), None)
    }

    pub fn emit_expression_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, None, Some(call_importer))
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
            let terminated = block_is_terminated(&builder, entry);
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

    fn emit_expression_inner<'services>(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: Option<&'services mut dyn StringInterner>,
        call_importer: Option<&'services mut dyn CallImporter>,
    ) -> Result<Function, FunctionEmissionError> {
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let value = lower_expression(
                &mut IsleContext::new_with_services(
                    &mut builder,
                    facts,
                    string_interner,
                    call_importer,
                ),
                body,
            )
            .map_err(FunctionEmissionError::Lowering)?;
            builder.ins().return_(&[value]);
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::Verification(error.to_string()))?;
        Ok(function)
    }
}

fn block_is_terminated(builder: &FunctionBuilder<'_>, block: Block) -> bool {
    builder
        .func
        .layout
        .last_inst(block)
        .is_some_and(|inst| builder.func.dfg.insts[inst].opcode().is_terminator())
}

#[derive(Debug)]
pub enum FunctionEmissionError {
    Lowering(LoweringError),
    Verification(String),
}
