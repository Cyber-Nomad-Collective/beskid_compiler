//! Generated ISLE selection for Beskid's stock CLIF lowering path.
//!
//! Raw constructors are an implementation detail of [`FunctionEmitter`]:
//!
//! ```compile_fail
//! use beskid_isle::generated;
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub use beskid_queries::AstNodeKey;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::types;
pub use cranelift_codegen::ir::{
    AbiParam, Block, ExternalName, FuncRef, Function, GlobalValueData, MemFlags, Signature, StackSlotData, StackSlotKind,
    TrapCode, Type, UserFuncName, Value,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::FunctionBuilder;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_frontend::Switch;
use cranelift_frontend::Variable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Program,
    FunctionDefinition,
    TestDefinition,
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
    FieldExpression,
    StructLiteralExpression,
    EnumLiteralExpression,
    MatchExpression,
    RangeExpression,
    BlockExpression,
    ForStatement,
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

/// Compiler-owned primitives available only to canonical runtime syntax.
///
/// They are selected from the manifest-backed capability, never from a user-declared extern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeIntrinsicKind {
    MemoryCopy,
    MemorySet,
    NativeWordFromPointer,
    PointerFromNativeWord,
    PointerAdd,
    RawWordLoad,
    RawWordStore,
    RawByteLoad,
    RawByteStore,
    TlsGet,
    TlsSet,
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

/// Exact semantic call target.
///
/// Source items carry their complete generation-safe syntax key.  A node id is only unique
/// within one source unit and revision, so using it as a module-import key can bind a call to an
/// unrelated item when two units happen to assign the same local id. Runtime intrinsics are not
/// source items and retain their canonical ABI-table index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirectCallee {
    Item(AstNodeKey),
    RuntimeIntrinsic(u32),
}

impl DirectCallee {
    pub const fn item(key: AstNodeKey) -> Self {
        Self::Item(key)
    }

    pub const fn runtime_intrinsic(index: u32) -> Self {
        Self::RuntimeIntrinsic(index)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    value_type: Type,
    offset: u32,
}

impl FieldLayout {
    pub const fn new(value_type: Type, offset: u32) -> Self {
        Self { value_type, offset }
    }
}

fn aggregate_field_is_valid(size: u32, alignment: u32, field: FieldLayout) -> bool {
    let field_size = field.value_type.bytes();
    let Some(end) = field.offset.checked_add(field_size) else {
        return false;
    };
    let field_alignment = field_size.next_power_of_two().min(alignment);
    field_size > 0 && end <= size && field.offset.is_multiple_of(field_alignment)
}

fn aggregate_fields_overlap(left: FieldLayout, right: FieldLayout) -> bool {
    let left_end = left.offset + left.value_type.bytes();
    let right_end = right.offset + right.value_type.bytes();
    left.offset < right_end && right.offset < left_end
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructLayout {
    size: u32,
    align_shift: u8,
    fields: Vec<FieldLayout>,
}

impl StructLayout {
    pub fn new(size: u32, align_shift: u8, fields: Vec<FieldLayout>) -> Self {
        Self {
            size,
            align_shift,
            fields,
        }
    }

    fn is_valid(&self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        if self.size == 0 || self.size > i32::MAX as u32 || !self.size.is_multiple_of(alignment) {
            return false;
        }
        for (index, field) in self.fields.iter().enumerate() {
            if !aggregate_field_is_valid(self.size, alignment, *field) {
                return false;
            }
            if self.fields[..index]
                .iter()
                .any(|other| aggregate_fields_overlap(*field, *other))
            {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumVariantLayout {
    discriminant: u64,
    payload: Option<FieldLayout>,
}

impl EnumVariantLayout {
    pub const fn new(discriminant: u64, payload: Option<FieldLayout>) -> Self {
        Self {
            discriminant,
            payload,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumLayout {
    size: u32,
    align_shift: u8,
    tag: FieldLayout,
    variants: Vec<EnumVariantLayout>,
}

impl EnumLayout {
    pub fn new(
        size: u32,
        align_shift: u8,
        tag: FieldLayout,
        variants: Vec<EnumVariantLayout>,
    ) -> Self {
        Self {
            size,
            align_shift,
            tag,
            variants,
        }
    }

    fn is_valid(&self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        if self.size == 0
            || self.size > i32::MAX as u32
            || !self.size.is_multiple_of(alignment)
            || !self.tag.value_type.is_int()
            || !aggregate_field_is_valid(self.size, alignment, self.tag)
            || self.variants.is_empty()
        {
            return false;
        }
        let tag_bits = self.tag.value_type.bits();
        if tag_bits > 64 {
            return false;
        }
        let mut discriminants = HashSet::with_capacity(self.variants.len());
        self.variants.iter().all(|variant| {
            let discriminant_fits = tag_bits == 64 || variant.discriminant < (1_u64 << tag_bits);
            let payload_is_valid = variant.payload.is_none_or(|payload| {
                aggregate_field_is_valid(self.size, alignment, payload)
                    && !aggregate_fields_overlap(self.tag, payload)
            });
            discriminant_fits && payload_is_valid && discriminants.insert(variant.discriminant)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchArmFact {
    discriminant: Option<u64>,
    body: AstNodeKey,
}

impl MatchArmFact {
    pub const fn variant(discriminant: u64, body: AstNodeKey) -> Self {
        Self {
            discriminant: Some(discriminant),
            body,
        }
    }

    pub const fn wildcard(body: AstNodeKey) -> Self {
        Self {
            discriminant: None,
            body,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeFact {
    start: AstNodeKey,
    end: AstNodeKey,
    step: i64,
    inclusive: bool,
}

impl RangeFact {
    pub const fn new(start: AstNodeKey, end: AstNodeKey, step: i64, inclusive: bool) -> Self {
        Self {
            start,
            end,
            step,
            inclusive,
        }
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
    fn runtime_intrinsic_kind(&self, _key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        None
    }
    fn child(&self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
        None
    }
    fn statement_count(&self, _key: AstNodeKey) -> Option<u8> {
        None
    }
    fn block_result(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
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
    fn string_literal(&self, _key: AstNodeKey) -> Option<Arc<str>> {
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
    fn struct_fields(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn struct_layout(&self, _key: AstNodeKey) -> Option<StructLayout> {
        None
    }
    fn field_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn enum_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    fn enum_variant_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn enum_payload(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn match_arms(&self, _key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        None
    }
    fn range_fact(&self, _key: AstNodeKey) -> Option<RangeFact> {
        None
    }
    fn local_slot(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    /// Parameter slots in source order for one function item.
    fn function_parameters(&self, _key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        None
    }
}

/// Generation-safe local slot and scalar type for one emitted function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterSlot {
    pub slot: u32,
    pub value_type: Type,
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
    InvalidStructLayout,
    InvalidStructField(u32),
    InvalidEnumLayout,
    InvalidEnumVariant(u32),
    InvalidMatchArms,
    NonExhaustiveMatch,
    InvalidBlockExpression,
    InvalidRangeFor,
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

    fn import_direct_call(
        &mut self,
        key: AstNodeKey,
    ) -> Option<(cranelift_codegen::ir::Inst, Signature)> {
        let callee = self.facts.direct_callee(key)?;
        let signature = self.facts.call_signature(key)?;
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
        Some((call, signature))
    }

    fn direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.returns.len() != 1 || signature.returns[0].value_type != result_type {
            return None;
        }
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).first().copied()
    }

    fn direct_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
        self.facts
            .call_signature(key)?
            .returns
            .is_empty()
            .then_some(())?;
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).is_empty().then_some(())
    }

    fn runtime_intrinsic_arguments(&mut self, key: AstNodeKey) -> Option<Vec<Value>> {
        self.facts
            .call_arguments(key)?
            .into_iter()
            .map(|argument| generated::constructor_lower_expression(self, argument))
            .collect()
    }

    fn emit_runtime_intrinsic_statement(&mut self, key: AstNodeKey) -> Option<()> {
        let Some(kind) = self.facts.runtime_intrinsic_kind(key) else {
            return self.direct_call_statement(key);
        };
        let arguments = self.runtime_intrinsic_arguments(key)?;
        match kind {
            RuntimeIntrinsicKind::RawWordStore | RuntimeIntrinsicKind::RawByteStore => {
                let [address, value] = arguments.as_slice() else { return None; };
                let pointer = self.builder.func.dfg.value_type(*address);
                if !pointer.is_int() {
                    return None;
                }
                self.builder.ins().store(MemFlags::new(), *value, *address, 0);
                Some(())
            }
            RuntimeIntrinsicKind::MemorySet => {
                let [destination, byte, length] = arguments.as_slice() else { return None; };
                self.emit_memory_set(*destination, *byte, *length)
            }
            RuntimeIntrinsicKind::MemoryCopy => {
                let [destination, source, length] = arguments.as_slice() else { return None; };
                self.emit_memory_copy(*destination, *source, *length)
            }
            RuntimeIntrinsicKind::TlsSet => {
                let [value] = arguments.as_slice() else { return None; };
                let pointer = self.builder.func.dfg.value_type(*value);
                if !pointer.is_int() { return None; }
                let address = self.runtime_tls_address(pointer);
                self.builder.ins().store(MemFlags::new(), *value, address, 0);
                Some(())
            }
            _ => self.direct_call_statement(key),
        }
    }

    fn emit_memory_set(&mut self, destination: Value, byte: Value, length: Value) -> Option<()> {
        let pointer = self.builder.func.dfg.value_type(destination);
        if !pointer.is_int()
            || self.builder.func.dfg.value_type(length) != pointer
            || self.builder.func.dfg.value_type(byte) != types::I8
        {
            return None;
        }
        let address = self.builder.declare_var(pointer);
        let remaining = self.builder.declare_var(pointer);
        self.builder.def_var(address, destination);
        self.builder.def_var(remaining, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let count = self.builder.use_var(remaining);
        let done = self.builder.ins().icmp_imm(IntCC::Equal, count, 0);
        self.builder.ins().brif(done, exit, &[], body, &[]);
        self.builder.switch_to_block(body);
        let current = self.builder.use_var(address);
        self.builder.ins().store(MemFlags::new(), byte, current, 0);
        let next_address = self.builder.ins().iadd_imm(current, 1);
        self.builder.def_var(address, next_address);
        let count = self.builder.use_var(remaining);
        let next_count = self.builder.ins().iadd_imm(count, -1);
        self.builder.def_var(remaining, next_count);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);
        self.builder.seal_block(body);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }

    fn emit_memory_copy(&mut self, destination: Value, source: Value, length: Value) -> Option<()> {
        let pointer = self.builder.func.dfg.value_type(destination);
        if !pointer.is_int()
            || self.builder.func.dfg.value_type(source) != pointer
            || self.builder.func.dfg.value_type(length) != pointer
        {
            return None;
        }
        let destination_var = self.builder.declare_var(pointer);
        let source_var = self.builder.declare_var(pointer);
        let remaining = self.builder.declare_var(pointer);
        self.builder.def_var(destination_var, destination);
        self.builder.def_var(source_var, source);
        self.builder.def_var(remaining, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let count = self.builder.use_var(remaining);
        let done = self.builder.ins().icmp_imm(IntCC::Equal, count, 0);
        self.builder.ins().brif(done, exit, &[], body, &[]);
        self.builder.switch_to_block(body);
        let source_address = self.builder.use_var(source_var);
        let byte = self.builder.ins().load(types::I8, MemFlags::new(), source_address, 0);
        let destination_address = self.builder.use_var(destination_var);
        self.builder.ins().store(MemFlags::new(), byte, destination_address, 0);
        let next_source = self.builder.ins().iadd_imm(source_address, 1);
        self.builder.def_var(source_var, next_source);
        let next_destination = self.builder.ins().iadd_imm(destination_address, 1);
        self.builder.def_var(destination_var, next_destination);
        let count = self.builder.use_var(remaining);
        let next_count = self.builder.ins().iadd_imm(count, -1);
        self.builder.def_var(remaining, next_count);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);
        self.builder.seal_block(body);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }

    fn runtime_tls_address(&mut self, pointer: Type) -> Value {
        let global = self.builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(b"__beskid_runtime_tls"),
            offset: 0.into(),
            colocated: true,
            tls: true,
        });
        self.builder.ins().global_value(pointer, global)
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

    fn assignment_target_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts
            .child(key, 0)
            .and_then(|target| self.facts.node_kind(target))
    }

    fn for_iterable_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts
            .child(key, 0)
            .and_then(|iterable| self.facts.node_kind(iterable))
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
            .intern(self.builder, key, &text)
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

    fn emit_expression_statement(&mut self, key: AstNodeKey) -> Option<()> {
        let expression = self.facts.child(key, 0)?;
        if self.facts.node_kind(expression) == Some(NodeKind::CallExpression) {
            if self.facts.call_kind(expression) == Some(CallKind::RuntimeIntrinsic) {
                return self.emit_runtime_intrinsic_statement(expression);
            }
            if self.facts.call_kind(expression) == Some(CallKind::Direct)
                && self
                    .facts
                    .call_signature(expression)
                    .is_some_and(|signature| signature.returns.is_empty())
            {
                return self.direct_call_statement(expression);
            }
        }
        let value = generated::constructor_lower_expression(self, expression)?;
        self.discard_value(value);
        Some(())
    }

    fn emit_runtime_intrinsic(&mut self, key: AstNodeKey) -> Option<Value> {
        let Some(kind) = self.facts.runtime_intrinsic_kind(key) else {
            return self.direct_call(key);
        };
        let arguments = self.runtime_intrinsic_arguments(key)?;
        let result = self.facts.scalar_type(key)?;
        match kind {
            RuntimeIntrinsicKind::NativeWordFromPointer
            | RuntimeIntrinsicKind::PointerFromNativeWord => {
                let [value] = arguments.as_slice() else { return None; };
                (self.builder.func.dfg.value_type(*value) == result).then_some(*value)
            }
            RuntimeIntrinsicKind::PointerAdd => {
                let [base, offset] = arguments.as_slice() else { return None; };
                (self.builder.func.dfg.value_type(*base) == result
                    && self.builder.func.dfg.value_type(*offset) == result)
                    .then(|| self.builder.ins().iadd(*base, *offset))
            }
            RuntimeIntrinsicKind::RawWordLoad => {
                let [address] = arguments.as_slice() else { return None; };
                (self.builder.func.dfg.value_type(*address) == result)
                    .then(|| self.builder.ins().load(result, MemFlags::new(), *address, 0))
            }
            RuntimeIntrinsicKind::RawByteLoad => {
                let [address] = arguments.as_slice() else { return None; };
                (self.builder.func.dfg.value_type(*address).is_int() && result == types::I8)
                    .then(|| self.builder.ins().load(result, MemFlags::new(), *address, 0))
            }
            RuntimeIntrinsicKind::TlsGet => {
                if !arguments.is_empty() { return None; }
                let address = self.runtime_tls_address(result);
                Some(self.builder.ins().load(result, MemFlags::new(), address, 0))
            }
            RuntimeIntrinsicKind::MemoryCopy
            | RuntimeIntrinsicKind::MemorySet
            | RuntimeIntrinsicKind::RawWordStore
            | RuntimeIntrinsicKind::RawByteStore
            | RuntimeIntrinsicKind::TlsSet => None,
        }
    }

    fn discard_value(&mut self, _value: Value) {}

    fn emit_block_expression(&mut self, key: AstNodeKey) -> Option<Value> {
        let saved_locals = self.locals.clone();
        let lowered = (|| {
            let count = self.facts.statement_count(key)?;
            for index in 0..count {
                let statement = self.facts.child(key, index)?;
                generated::constructor_lower_statement(self, statement)?;
                let current = self.builder.current_block()?;
                if block_is_terminated(self.builder, current) {
                    self.pending_error = Some(LoweringError {
                        key,
                        kind: LoweringErrorKind::InvalidBlockExpression,
                    });
                    return None;
                }
            }
            let result_key = self.facts.block_result(key)?;
            let value = generated::constructor_lower_expression(self, result_key)?;
            if self.builder.func.dfg.value_type(value) != self.facts.scalar_type(key)? {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidBlockExpression,
                });
                return None;
            }
            Some(value)
        })();
        self.locals = saved_locals;
        lowered
    }

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
        let else_key = self.facts.child(key, 2);
        let condition = generated::constructor_lower_expression(self, condition_key)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(condition, then_block, &[], else_block, &[]);

        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        generated::constructor_lower_statement(self, then_key)?;
        if !block_is_terminated(self.builder, then_block) {
            self.builder.ins().jump(merge_block, &[]);
        }

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        if let Some(else_key) = else_key {
            generated::constructor_lower_statement(self, else_key)?;
        }
        if !block_is_terminated(self.builder, else_block) {
            self.builder.ins().jump(merge_block, &[]);
        }
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
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

    fn emit_range_for(&mut self, key: AstNodeKey) -> Option<()> {
        let iterable = self.facts.child(key, 0)?;
        let body_key = self.facts.child(key, 1)?;
        let range = self.facts.range_fact(iterable)?;
        let iterator_type = self.facts.scalar_type(key)?;
        let slot = self.facts.local_slot(key)?;
        if range.step == 0 || !iterator_type.is_int() || self.locals.contains_key(&slot) {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidRangeFor,
            });
            return None;
        }
        let start = generated::constructor_lower_expression(self, range.start)?;
        let end = generated::constructor_lower_expression(self, range.end)?;
        if self.builder.func.dfg.value_type(start) != iterator_type
            || self.builder.func.dfg.value_type(end) != iterator_type
        {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidRangeFor,
            });
            return None;
        }
        let iterator = self.builder.declare_var(iterator_type);
        self.builder.def_var(iterator, start);
        self.locals.insert(slot, (iterator, iterator_type));

        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let latch = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let current = self.builder.use_var(iterator);
        let condition = match (range.step.is_positive(), range.inclusive) {
            (true, false) => self.builder.ins().icmp(IntCC::SignedLessThan, current, end),
            (true, true) => self
                .builder
                .ins()
                .icmp(IntCC::SignedLessThanOrEqual, current, end),
            (false, false) => self
                .builder
                .ins()
                .icmp(IntCC::SignedGreaterThan, current, end),
            (false, true) => self
                .builder
                .ins()
                .icmp(IntCC::SignedGreaterThanOrEqual, current, end),
        };
        self.builder.ins().brif(condition, body, &[], exit, &[]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        self.loop_stack.push(LoopTargets {
            continue_block: latch,
            break_block: exit,
        });
        let lowered = generated::constructor_lower_statement(self, body_key);
        self.loop_stack.pop();
        if lowered.is_none() {
            self.locals.remove(&slot);
            return None;
        }
        if !block_is_terminated(self.builder, body) {
            self.builder.ins().jump(latch, &[]);
        }

        self.builder.switch_to_block(latch);
        self.builder.seal_block(latch);
        let current = self.builder.use_var(iterator);
        let next = self.builder.ins().iadd_imm(current, range.step);
        self.builder.def_var(iterator, next);
        self.builder.ins().jump(header, &[]);

        self.builder.seal_block(header);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        self.locals.remove(&slot);
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

    fn emit_struct_literal(&mut self, key: AstNodeKey) -> Option<Value> {
        let fields = self.facts.struct_fields(key)?;
        let layout = self.facts.struct_layout(key)?;
        if !layout.is_valid() || fields.len() != layout.fields.len() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.size,
            layout.align_shift,
        ));
        for (field_key, field_layout) in fields.into_iter().zip(layout.fields) {
            let value = generated::constructor_lower_expression(self, field_key)?;
            if self.builder.func.dfg.value_type(value) != field_layout.value_type {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidStructLayout,
                });
                return None;
            }
            self.builder
                .ins()
                .stack_store(value, slot, i32::try_from(field_layout.offset).ok()?);
        }
        let pointer_type = self.facts.scalar_type(key)?;
        if !pointer_type.is_int() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        Some(self.builder.ins().stack_addr(pointer_type, slot, 0))
    }

    fn emit_field_read(&mut self, key: AstNodeKey) -> Option<Value> {
        let layout = self.facts.struct_layout(key)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        let field_index = self.facts.field_index(key)?;
        let Some(field) = usize::try_from(field_index)
            .ok()
            .and_then(|index| layout.fields.get(index))
            .copied()
        else {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructField(field_index),
            });
            return None;
        };
        if self.facts.scalar_type(key)? != field.value_type {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        let base_key = self.facts.child(key, 0)?;
        let base = generated::constructor_lower_expression(self, base_key)?;
        if !self.builder.func.dfg.value_type(base).is_int() {
            return None;
        }
        Some(self.builder.ins().load(
            field.value_type,
            MemFlags::new(),
            base,
            i32::try_from(field.offset).ok()?,
        ))
    }

    fn emit_field_assign(&mut self, key: AstNodeKey) -> Option<Value> {
        let target = self.facts.child(key, 0)?;
        let value_key = self.facts.child(key, 1)?;
        let layout = self.facts.struct_layout(target)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        let field_index = self.facts.field_index(target)?;
        let Some(field) = usize::try_from(field_index)
            .ok()
            .and_then(|index| layout.fields.get(index))
            .copied()
        else {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructField(field_index),
            });
            return None;
        };
        let base_key = self.facts.child(target, 0)?;
        let base = generated::constructor_lower_expression(self, base_key)?;
        let value = generated::constructor_lower_expression(self, value_key)?;
        if !self.builder.func.dfg.value_type(base).is_int()
            || self.builder.func.dfg.value_type(value) != field.value_type
            || self.facts.scalar_type(key)? != field.value_type
        {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidStructLayout,
            });
            return None;
        }
        self.builder.ins().store(
            MemFlags::new(),
            value,
            base,
            i32::try_from(field.offset).ok()?,
        );
        Some(value)
    }

    fn emit_enum_literal(&mut self, key: AstNodeKey) -> Option<Value> {
        let layout = self.facts.enum_layout(key)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidEnumLayout,
            });
            return None;
        }
        let variant_index = self.facts.enum_variant_index(key)?;
        let Some(variant) = usize::try_from(variant_index)
            .ok()
            .and_then(|index| layout.variants.get(index))
            .copied()
        else {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidEnumVariant(variant_index),
            });
            return None;
        };
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.size,
            layout.align_shift,
        ));
        let tag = self
            .builder
            .ins()
            .iconst(layout.tag.value_type, variant.discriminant as i64);
        self.builder
            .ins()
            .stack_store(tag, slot, i32::try_from(layout.tag.offset).ok()?);
        match (variant.payload, self.facts.enum_payload(key)) {
            (Some(payload_layout), Some(payload_key)) => {
                let payload = generated::constructor_lower_expression(self, payload_key)?;
                if self.builder.func.dfg.value_type(payload) != payload_layout.value_type {
                    self.pending_error = Some(LoweringError {
                        key,
                        kind: LoweringErrorKind::InvalidEnumLayout,
                    });
                    return None;
                }
                self.builder.ins().stack_store(
                    payload,
                    slot,
                    i32::try_from(payload_layout.offset).ok()?,
                );
            }
            (None, None) => {}
            _ => {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidEnumLayout,
                });
                return None;
            }
        }
        let pointer_type = self.facts.scalar_type(key)?;
        if !pointer_type.is_int() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidEnumLayout,
            });
            return None;
        }
        Some(self.builder.ins().stack_addr(pointer_type, slot, 0))
    }

    fn emit_match(&mut self, key: AstNodeKey) -> Option<Value> {
        let layout = self.facts.enum_layout(key)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidEnumLayout,
            });
            return None;
        }
        let arms = self.facts.match_arms(key)?;
        if arms.is_empty() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidMatchArms,
            });
            return None;
        }
        let layout_discriminants = layout
            .variants
            .iter()
            .map(|variant| variant.discriminant)
            .collect::<HashSet<_>>();
        let mut covered = HashSet::with_capacity(arms.len());
        let wildcard_index = arms.iter().position(|arm| arm.discriminant.is_none());
        if wildcard_index.is_some_and(|index| index + 1 != arms.len())
            || arms.iter().filter(|arm| arm.discriminant.is_none()).count() > 1
            || arms
                .iter()
                .filter_map(|arm| arm.discriminant)
                .any(|tag| !layout_discriminants.contains(&tag) || !covered.insert(tag))
        {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidMatchArms,
            });
            return None;
        }
        if wildcard_index.is_none() && covered != layout_discriminants {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::NonExhaustiveMatch,
            });
            return None;
        }

        let scrutinee_key = self.facts.child(key, 0)?;
        let scrutinee = generated::constructor_lower_expression(self, scrutinee_key)?;
        if !self.builder.func.dfg.value_type(scrutinee).is_int() {
            self.pending_error = Some(LoweringError {
                key,
                kind: LoweringErrorKind::InvalidEnumLayout,
            });
            return None;
        }
        let tag = self.builder.ins().load(
            layout.tag.value_type,
            MemFlags::new(),
            scrutinee,
            i32::try_from(layout.tag.offset).ok()?,
        );
        let result_type = self.facts.scalar_type(key)?;
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, result_type);
        let arm_blocks = arms
            .iter()
            .map(|_| self.builder.create_block())
            .collect::<Vec<_>>();
        let trap = wildcard_index
            .is_none()
            .then(|| self.builder.create_block());
        let default = wildcard_index.map_or_else(
            || trap.expect("trap block exists without wildcard"),
            |index| arm_blocks[index],
        );
        let mut switch = Switch::new();
        for (arm, block) in arms.iter().zip(&arm_blocks) {
            if let Some(discriminant) = arm.discriminant {
                switch.set_entry(u128::from(discriminant), *block);
            }
        }
        switch.emit(self.builder, tag, default);

        for (arm, block) in arms.into_iter().zip(arm_blocks) {
            self.builder.switch_to_block(block);
            self.builder.seal_block(block);
            let value = generated::constructor_lower_expression(self, arm.body)?;
            if self.builder.func.dfg.value_type(value) != result_type {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidMatchArms,
                });
                return None;
            }
            self.builder.ins().jump(merge, &[value.into()]);
        }
        if let Some(trap) = trap {
            self.builder.switch_to_block(trap);
            self.builder.seal_block(trap);
            self.builder.ins().trap(TrapCode::unwrap_user(1));
        }
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge).first().copied()
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
        self.emit_statement_inner(name, signature, facts, None, body, None, None)
    }

    /// Emit a parsed function item after binding its source parameters to local slots.
    pub fn emit_item_statement(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: AstNodeKey,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(name, signature, facts, Some(item), body, None, None)
    }

    fn emit_statement_inner<'services>(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: Option<AstNodeKey>,
        body: AstNodeKey,
        string_interner: Option<&'services mut dyn StringInterner>,
        call_importer: Option<&'services mut dyn CallImporter>,
    ) -> Result<Function, FunctionEmissionError> {
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let mut context =
                IsleContext::new_with_services(&mut builder, facts, string_interner, call_importer);
            if let Some(item) = item {
                materialize_parameters(&mut context, item)?;
            }
            lower_statement(&mut context, body).map_err(FunctionEmissionError::Lowering)?;
            let terminated = block_is_terminated(&builder, entry);
            if !terminated {
                if builder.func.signature.returns.is_empty() {
                    builder.ins().return_(&[]);
                } else {
                    return Err(FunctionEmissionError::Verification(
                        "generated statement body did not terminate its entry block".to_owned(),
                    ));
                }
            }
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::Verification(error.to_string()))?;
        Ok(function)
    }

    pub fn emit_statement_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            name,
            signature,
            facts,
            None,
            body,
            None,
            Some(call_importer),
        )
    }

    /// Emit a parsed function item with parameter materialization and explicit call imports.
    pub fn emit_item_statement_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: AstNodeKey,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            name,
            signature,
            facts,
            Some(item),
            body,
            None,
            Some(call_importer),
        )
    }

    /// Emit a parsed item with both artifact-owned string interning and exact call imports.
    pub fn emit_item_statement_with_services(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: AstNodeKey,
        body: AstNodeKey,
        string_interner: &mut dyn StringInterner,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            name,
            signature,
            facts,
            Some(item),
            body,
            Some(string_interner),
            Some(call_importer),
        )
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

fn materialize_parameters(
    context: &mut IsleContext<'_, '_, '_, '_>,
    item: AstNodeKey,
) -> Result<(), FunctionEmissionError> {
    let parameters = context.facts.function_parameters(item).ok_or_else(|| {
        FunctionEmissionError::Verification("item parameter facts are unavailable".to_owned())
    })?;
    let incoming = context
        .builder
        .block_params(context.builder.current_block().ok_or_else(|| {
            FunctionEmissionError::Verification("function has no entry block".to_owned())
        })?)
        .to_vec();
    if parameters.len() != incoming.len() {
        return Err(FunctionEmissionError::Verification(
            "item parameter facts do not match function signature".to_owned(),
        ));
    }
    for (parameter, value) in parameters.into_iter().zip(incoming) {
        if context.locals.contains_key(&parameter.slot)
            || context.builder.func.dfg.value_type(value) != parameter.value_type
        {
            return Err(FunctionEmissionError::Verification(
                "item parameter slot or type is invalid".to_owned(),
            ));
        }
        let variable = context.builder.declare_var(parameter.value_type);
        context.builder.def_var(variable, value);
        context
            .locals
            .insert(parameter.slot, (variable, parameter.value_type));
    }
    Ok(())
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
