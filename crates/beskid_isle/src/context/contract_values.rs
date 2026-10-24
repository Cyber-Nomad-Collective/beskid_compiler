//! Contract values (runtime dispatch, v0.6.7): a concrete value written into a contract-typed
//! destination is boxed with its witness table; a contract method call without a static target
//! loads the receiver word and the table entry out of the box and calls it indirectly.

use super::*;
use crate::{ContractBoxPlan, ContractBoxReceiverFact};

/// Byte offset of the receiver word inside a contract box.
pub const CONTRACT_BOX_RECEIVER_OFFSET: i32 = 16;
/// Byte offset of the witness table address inside a contract box.
pub const CONTRACT_BOX_WITNESS_OFFSET: i32 = 24;

impl IsleContext<'_, '_, '_, '_> {
    /// Lower the expression at `key` for a destination: when it is a concrete value of a
    /// contract-typed destination, the result is its contract box.
    pub(in crate::context) fn lower_coerced_expression(&mut self, key: AstNodeKey) -> Option<Value> {
        let value = self.lower_nested_expression(key)?;
        self.coerce_to_destination(key, value)
    }

    /// Box `value` (just lowered from `key`) when its destination is a contract type.
    pub(in crate::context) fn coerce_to_destination(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
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

    /// The receiver word of a primitive: integers are extended to the word (signed types by
    /// sign, `bool`, `char` and unsigned types by zero), floats keep their bit pattern.
    fn widen_receiver_word(&mut self, value: Value, ty: Type, pointer: Type) -> Option<Value> {
        let actual = self.builder.func.dfg.value_type(value);
        (actual == ty).then_some(())?;
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
        let entry_offset = i32::try_from(u64::from(plan.slot) * 8).ok()?;
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
