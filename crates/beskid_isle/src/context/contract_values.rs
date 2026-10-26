//! Contract values (runtime dispatch, v0.6.7): a concrete value written into a contract-typed
//! destination is boxed with its witness table; a contract method call without a static target
//! loads the receiver word and the table entry out of the box and calls it indirectly.

use super::*;
use crate::{ContractBoxPlan, ContractBoxReceiverFact, ContractUpcastPlan};

/// Byte offset of the receiver word inside a contract box.
pub const CONTRACT_BOX_RECEIVER_OFFSET: i32 = 16;
/// Byte offset of the witness table address inside a contract box.
pub const CONTRACT_BOX_WITNESS_OFFSET: i32 = 24;
/// Words of a witness table before its method entries: the type tag, then the receiver kind.
pub const WITNESS_HEADER_WORDS: i32 = 2;
/// Byte offset of the type tag inside a witness table.
const WITNESS_TYPE_TAG_OFFSET: i32 = 0;
/// Byte offset of the receiver kind inside a witness table.
const WITNESS_RECEIVER_KIND_OFFSET: i32 = 8;
/// Receiver kind of a managed reference receiver word.
const WITNESS_RECEIVER_REFERENCE: i64 = 0;
/// Receiver kind of an `f32` receiver word.
const WITNESS_RECEIVER_F32: i64 = 2;
/// Receiver kind of an `f64` receiver word.
const WITNESS_RECEIVER_F64: i64 = 3;

impl IsleContext<'_, '_, '_, '_> {
    /// Lower the expression at `key` for a destination: when it is a concrete value of a
    /// contract-typed destination, the result is its contract box.
    pub(in crate::context) fn lower_coerced_expression(&mut self, key: AstNodeKey) -> Option<Value> {
        let value = self.lower_nested_expression(key)?;
        self.coerce_to_destination(key, value)
    }

    /// Box `value` (just lowered from `key`) when its destination is a contract type.
    pub(crate) fn coerce_to_destination(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
        if let Some(plan) = self.facts.contract_upcast(key) {
            return self.emit_contract_upcast(key, value, &plan);
        }
        let Some(plan) = self.facts.contract_box(key) else {
            return Some(value);
        };
        if self.facts.nonallocating_publication_body() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::ValueCopyInAllocationFreeBody });
            return None;
        }
        let root = match plan.receiver {
            ContractBoxReceiverFact::Reference => self.root_expression_value_if_needed(key, value)?,
            ContractBoxReceiverFact::Scalar(_) => None,
        };
        let object = self.emit_contract_box(value, &plan)?;
        self.release_expression_root(root)?;
        Some(object)
    }

    /// Whether the value lowered from `key` becomes a new contract box at its destination (a
    /// boxed concrete value or a contract value converted to an embedded contract): the result
    /// is then a managed reference whatever `key`'s own type is.
    pub(in crate::context) fn converts_to_contract_box(&self, key: AstNodeKey) -> bool {
        self.facts.contract_box(key).is_some() || self.facts.contract_upcast(key).is_some()
    }

    /// Convert the contract box `source` (lowered from `key`) to an embedded contract of its
    /// contract: a new box with the same receiver word and the embedded contract's table, read
    /// from the source table at `plan.table_word`. The receiver kind of that table selects the
    /// traced or the scalar box descriptor.
    fn emit_contract_upcast(&mut self, key: AstNodeKey, source: Value, plan: &ContractUpcastPlan) -> Option<Value> {
        if self.facts.nonallocating_publication_body() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::ValueCopyInAllocationFreeBody });
            return None;
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        (self.builder.func.dfg.value_type(source) == pointer).then_some(())?;
        self.builder.ins().trapz(source, TrapCode::unwrap_user(5));
        let root = self.root_temporary(source)?;
        let source_table = self.builder.ins().load(pointer, MemFlagsData::new(), source, CONTRACT_BOX_WITNESS_OFFSET);
        let table_offset = i32::try_from(u64::from(plan.table_word) * 8).ok()?;
        let table = self.builder.ins().load(pointer, MemFlagsData::new(), source_table, table_offset);
        let kind = self.builder.ins().load(pointer, MemFlagsData::new(), table, WITNESS_RECEIVER_KIND_OFFSET);
        let reference_request = self.symbol_global(plan.reference_request_symbol.as_ref(), pointer)?;
        let scalar_request = self.symbol_global(plan.scalar_request_symbol.as_ref(), pointer)?;
        let traced = self.builder.ins().icmp_imm(IntCC::Equal, kind, WITNESS_RECEIVER_REFERENCE);
        let request = self.builder.ins().select(traced, reference_request, scalar_request);
        let allocate = self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
        let call = self.builder.ins().call(allocate, &[request]);
        let object = self.builder.inst_results(call).first().copied()?;
        self.guard_checked_allocation()?;
        self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
        // The source box stays rooted across the allocation; its receiver word is read after it.
        let word = self.builder.ins().load(pointer, MemFlagsData::new(), source, CONTRACT_BOX_RECEIVER_OFFSET);
        // Initialization stores into the fresh object: no barrier.
        self.builder.ins().store(MemFlagsData::new(), word, object, CONTRACT_BOX_RECEIVER_OFFSET);
        self.builder.ins().store(MemFlagsData::new(), table, object, CONTRACT_BOX_WITNESS_OFFSET);
        self.release_temporary_root(Some(root))?;
        Some(object)
    }

    /// `==` (or `!=` when `invert`) on two contract values: equal exactly when both hold the
    /// same implementing value. The witness tables' type tags must match, and then the receiver
    /// words: bit equality for references (object identity) and integer, `bool` or `char` bits,
    /// IEEE equality for `f32` and `f64` (so `NaN` differs from itself and `0.0` equals `-0.0`).
    pub(in crate::context) fn lower_contract_value_equality(&mut self, key: AstNodeKey, invert: bool) -> Option<Value> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = generated::constructor_lower_expression(self, left_key)?;
        (self.builder.func.dfg.value_type(left) == pointer).then_some(())?;
        let left_root = self.root_expression_value_if_needed(left_key, left)?;
        let right = generated::constructor_lower_expression(self, right_key)?;
        (self.builder.func.dfg.value_type(right) == pointer).then_some(())?;
        self.release_expression_root(left_root)?;
        self.builder.ins().trapz(left, TrapCode::unwrap_user(5));
        self.builder.ins().trapz(right, TrapCode::unwrap_user(5));
        let load = |context: &mut Self, base: Value, offset: i32| {
            context.builder.ins().load(pointer, MemFlagsData::new(), base, offset)
        };
        let left_table = load(self, left, CONTRACT_BOX_WITNESS_OFFSET);
        let right_table = load(self, right, CONTRACT_BOX_WITNESS_OFFSET);
        let left_tag = load(self, left_table, WITNESS_TYPE_TAG_OFFSET);
        let right_tag = load(self, right_table, WITNESS_TYPE_TAG_OFFSET);
        let kind = load(self, left_table, WITNESS_RECEIVER_KIND_OFFSET);
        let left_word = load(self, left, CONTRACT_BOX_RECEIVER_OFFSET);
        let right_word = load(self, right, CONTRACT_BOX_RECEIVER_OFFSET);
        let same_type = self.builder.ins().icmp(IntCC::Equal, left_tag, right_tag);
        let same_bits = self.builder.ins().icmp(IntCC::Equal, left_word, right_word);
        let same_value = if pointer == types::I64 {
            let left_f64 = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), left_word);
            let right_f64 = self.builder.ins().bitcast(types::F64, MemFlagsData::new(), right_word);
            let equal_f64 = self.builder.ins().fcmp(FloatCC::Equal, left_f64, right_f64);
            let left_low = self.builder.ins().ireduce(types::I32, left_word);
            let right_low = self.builder.ins().ireduce(types::I32, right_word);
            let left_f32 = self.builder.ins().bitcast(types::F32, MemFlagsData::new(), left_low);
            let right_f32 = self.builder.ins().bitcast(types::F32, MemFlagsData::new(), right_low);
            let equal_f32 = self.builder.ins().fcmp(FloatCC::Equal, left_f32, right_f32);
            let is_f64 = self.builder.ins().icmp_imm(IntCC::Equal, kind, WITNESS_RECEIVER_F64);
            let is_f32 = self.builder.ins().icmp_imm(IntCC::Equal, kind, WITNESS_RECEIVER_F32);
            let float_or_bits = self.builder.ins().select(is_f32, equal_f32, same_bits);
            self.builder.ins().select(is_f64, equal_f64, float_or_bits)
        } else {
            same_bits
        };
        let equal = self.builder.ins().band(same_type, same_value);
        Some(if invert { self.builder.ins().bxor_imm(equal, 1) } else { equal })
    }

    /// Allocate a contract box for `value` and `plan`. The caller keeps `value` rooted across
    /// the allocation when it is a managed reference.
    pub(in crate::context) fn emit_contract_box(&mut self, value: Value, plan: &ContractBoxPlan) -> Option<Value> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let word = match plan.receiver {
            ContractBoxReceiverFact::Reference => {
                (self.builder.func.dfg.value_type(value) == pointer).then_some(value)?
            }
            ContractBoxReceiverFact::Scalar(ty) => self.widen_receiver_word(value, ty, pointer)?,
        };
        let request = self.symbol_global(plan.allocation_request_symbol.as_ref(), pointer)?;
        let allocate = self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
        let call = self.builder.ins().call(allocate, &[request]);
        let object = self.builder.inst_results(call).first().copied()?;
        self.guard_checked_allocation()?;
        self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
        // Initialization stores into the fresh object: no barrier.
        self.builder.ins().store(MemFlagsData::new(), word, object, CONTRACT_BOX_RECEIVER_OFFSET);
        let witness = self.symbol_global(plan.witness_symbol.as_ref(), pointer)?;
        self.builder.ins().store(MemFlagsData::new(), witness, object, CONTRACT_BOX_WITNESS_OFFSET);
        Some(object)
    }

    /// The receiver word of a primitive: integers, `bool` and `char` are zero-extended to the
    /// word, floats keep their bit pattern; the witness thunk narrows the word back.
    fn widen_receiver_word(&mut self, value: Value, ty: Type, pointer: Type) -> Option<Value> {
        let actual = self.builder.func.dfg.value_type(value);
        // The box's receiver word is 8 bytes: `i64` and `f64` receivers need a 64-bit target.
        (actual == ty && pointer == types::I64 && ty.bits() <= pointer.bits()).then_some(())?;
        if ty == pointer {
            return Some(value);
        }
        let bits = if ty.is_float() {
            let int = Type::int(u16::try_from(ty.bits()).ok()?)?;
            self.builder.ins().bitcast(int, MemFlagsData::new(), value)
        } else {
            value
        };
        if self.builder.func.dfg.value_type(bits) == pointer {
            return Some(bits);
        }
        Some(self.builder.ins().uextend(pointer, bits))
    }

    /// The receiver word read out of the contract box `object` (conf2's static selection on a
    /// contract value calls the implementation directly with it).
    pub(in crate::context) fn unbox_contract_receiver(&mut self, object: Value) -> Value {
        let pointer = dispatch::pointer_type(self.frontend_config);
        self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
        self.builder.ins().load(pointer, MemFlagsData::new(), object, CONTRACT_BOX_RECEIVER_OFFSET)
    }

    /// A contract method call through the box's witness table.
    pub(in crate::context) fn contract_dispatch_call(&mut self, key: AstNodeKey, value: bool) -> Option<Option<Value>> {
        let plan = self.facts.contract_dispatch(key)?;
        let returns = plan.signature.returns.first().map(|result| result.value_type);
        (!value || returns.is_some()).then_some(())?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let argument_keys = self.facts.call_arguments(key)?;
        let (receiver_key, argument_keys) = argument_keys.split_first()?;
        let record = generated::constructor_lower_expression(self, *receiver_key)?;
        (self.builder.func.dfg.value_type(record) == pointer).then_some(())?;
        let record_root = self.root_expression_value_if_needed(*receiver_key, record)?;
        let mut arguments = Vec::with_capacity(argument_keys.len() + 1);
        let mut roots = Vec::with_capacity(argument_keys.len());
        let mut parameters = plan.signature.params.iter().skip(1);
        for argument in argument_keys.iter().copied() {
            if self.is_unit_value(argument) {
                self.lower_expression_for_effect(argument)?;
                continue;
            }
            let parameter = parameters.next()?.value_type;
            let value = self.lower_coerced_expression(argument)?;
            let value = if self.builder.func.dfg.value_type(value) == parameter {
                value
            } else {
                self.adapt_scalar_boundary(argument, value, parameter)?
            };
            roots.push(self.root_expression_value_if_needed(argument, value)?);
            arguments.push(value);
        }
        if parameters.next().is_some() {
            return None;
        }
        self.builder.ins().trapz(record, TrapCode::unwrap_user(5));
        let receiver = self.builder.ins().load(pointer, MemFlagsData::new(), record, CONTRACT_BOX_RECEIVER_OFFSET);
        let witness = self.builder.ins().load(pointer, MemFlagsData::new(), record, CONTRACT_BOX_WITNESS_OFFSET);
        let entry_offset = i32::try_from((u64::from(plan.slot) + WITNESS_HEADER_WORDS as u64) * 8).ok()?;
        let entry = self.builder.ins().load(pointer, MemFlagsData::new(), witness, entry_offset);
        let signature = self.builder.func.import_signature(plan.signature.clone());
        let mut call_arguments = Vec::with_capacity(arguments.len() + 1);
        call_arguments.push(receiver);
        call_arguments.extend(arguments);
        let call = self.builder.ins().call_indirect(signature, entry, &call_arguments);
        let result = self.builder.inst_results(call).first().copied();
        self.guard_checked_allocation()?;
        for root in roots.into_iter().rev() {
            self.release_expression_root(root)?;
        }
        self.release_expression_root(record_root)?;
        Some(result)
    }
}
