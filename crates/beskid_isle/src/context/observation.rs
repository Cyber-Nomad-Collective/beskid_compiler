use beskid_abi::runtime_source::{
    OBSERVER_NODE_CLOSURE_OFFSET, OBSERVER_NODE_DESCRIPTOR_SYMBOL, OBSERVER_NODE_NEXT_OFFSET,
    OBSERVER_NODE_PROPERTY_OFFSET,
};
use cranelift_codegen::ir::BlockArg;

use super::*;
use crate::facts::{ObservationEquality, ObservedDerived, ObservedStore};

struct ObservedChange {
    property_id: u64,
    previous: Value,
    next: Value,
    changed: Option<Value>,
}

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn emit_observe(&mut self, key: AstNodeKey) -> Option<()> {
        if let Some((receiver, callback, offset)) = self.facts.contract_observation(key) {
            let pointer = self.frontend_config.pointer_type();
            let record = generated::constructor_lower_expression(self, receiver)?;
            self.builder.ins().trapz(record, TrapCode::unwrap_user(5));
            let object = self.builder.ins().load(
                pointer,
                MemFlagsData::new(),
                record,
                super::contract_values::CONTRACT_BOX_RECEIVER_OFFSET,
            );
            let witness = self.builder.ins().load(
                pointer,
                MemFlagsData::new(),
                record,
                super::contract_values::CONTRACT_BOX_WITNESS_OFFSET,
            );
            let offset = i32::try_from(offset).ok()?;
            let slot = self.builder.ins().load(pointer, MemFlagsData::new(), witness, offset);
            self.builder.ins().trapz(slot, TrapCode::unwrap_user(5));
            let property = self.builder.ins().load(pointer, MemFlagsData::new(), witness, offset.checked_add(8)?);
            return self.register_observer(object, callback, slot, property);
        }
        let plan = self.facts.observation(key)?;
        let pointer = self.frontend_config.pointer_type();
        (pointer == types::I64).then_some(())?;
        let receiver = if self.facts.node_kind(plan.receiver) == Some(NodeKind::MethodDefinition) {
            let slot = self.facts.local_slot(plan.receiver)?;
            self.builder.use_var(self.locals.get(&slot)?.variable)
        } else {
            generated::constructor_lower_expression(self, plan.receiver)?
        };
        let slot = self.builder.ins().iconst(pointer, i64::from(plan.slot_offset));
        let property = self.builder.ins().iconst(pointer, plan.property_id as i64);
        self.register_observer(receiver, plan.callback, slot, property)
    }

    fn register_observer(&mut self, receiver: Value, callback: AstNodeKey, slot: Value, property: Value) -> Option<()> {
        let pointer = self.frontend_config.pointer_type();
        let receiver_root = self.root_expression_value(receiver)?;
        let callback = generated::constructor_lower_expression(self, callback)?;
        let callback_root = self.root_expression_value(callback)?;
        let descriptor = self.symbol_global(OBSERVER_NODE_DESCRIPTOR_SYMBOL, pointer)?;
        let subscribe = self.import_runtime_helper("observer_subscribe", &[pointer; 5], Some(types::I8))?;
        let call = self.builder.ins().call(subscribe, &[receiver, slot, property, callback, descriptor]);
        self.guard_checked_allocation()?;
        let registered = self.builder.inst_results(call)[0];
        self.builder.ins().trapz(registered, TrapCode::unwrap_user(5));
        self.release_expression_root(Some(callback_root))?;
        self.release_expression_root(Some(receiver_root))
    }

    pub(super) fn emit_observed_store(
        &mut self,
        key: AstNodeKey,
        receiver: Value,
        field_offset: i32,
        value: Value,
        managed: bool,
        plan: ObservedStore,
    ) -> Option<()> {
        let pointer = self.frontend_config.pointer_type();
        (pointer == types::I64).then_some(())?;
        let slot_offset = i32::try_from(plan.slot_offset).ok()?;
        let exit = self.builder.create_block();
        let mut changes = Vec::with_capacity(plan.derived.len() + usize::from(plan.property.is_some()));
        let mut backing_roots = Vec::new();
        // Before the write a managed new value may only be reachable in this frame.
        if managed {
            backing_roots.push(self.root_expression_value(value)?);
        }
        if let Some(property) = plan.property {
            let ty = self.builder.func.dfg.value_type(value);
            let previous = self.builder.ins().load(ty, MemFlagsData::new(), receiver, field_offset);
            if managed {
                backing_roots.push(self.root_expression_value(previous)?);
            }
            if !matches!(property.equality, ObservationEquality::Always) {
                let equal = self.observation_equal(key, &property.equality, previous, value)?;
                let changed = self.builder.create_block();
                self.builder.ins().brif(equal, exit, &[], changed, &[]);
                self.builder.switch_to_block(changed);
                self.builder.seal_block(changed);
            }
            changes.push(ObservedChange { property_id: property.property_id, previous, next: value, changed: None });
        }
        let capture = self.builder.create_block();
        let empty = if plan.derived.is_empty() {
            self.emit_slot_store(receiver, field_offset, value, managed)?;
            exit
        } else {
            self.builder.create_block()
        };
        let head = self.builder.ins().load(pointer, MemFlagsData::new(), receiver, slot_offset);
        self.builder.ins().brif(head, capture, &[], empty, &[]);
        if empty != exit {
            self.builder.switch_to_block(empty);
            self.builder.seal_block(empty);
            self.emit_slot_store(receiver, field_offset, value, managed)?;
            self.builder.ins().jump(exit, &[]);
        }
        self.builder.switch_to_block(capture);
        self.builder.seal_block(capture);
        self.builder.declare_value_needs_stack_map(head);

        let mut derived_roots = Vec::new();
        let tail = if plan.derived.is_empty() {
            let tail_function = self.import_runtime_helper("observer_tail", &[pointer], Some(pointer))?;
            let call = self.builder.ins().call(tail_function, &[head]);
            self.builder.inst_results(call)[0]
        } else {
            let (tail, subscribed) = self.observed_subscriptions(head, &plan.derived);
            let mut snapshot = |context: &mut Self| {
                plan.derived
                    .iter()
                    .zip(&subscribed)
                    .map(|(getter, &subscribed)| {
                        let value = context.capture_observed_getter(key, receiver, getter, subscribed)?;
                        if getter.managed {
                            derived_roots.push(context.root_expression_value(value)?);
                        }
                        Some(value)
                    })
                    .collect::<Option<Vec<_>>>()
            };
            let previous = snapshot(self)?;
            self.emit_slot_store(receiver, field_offset, value, managed)?;
            // Equality may call user code. Freeze every new value before the first comparison.
            let next = snapshot(self)?;
            for (((getter, previous), next), subscribed) in plan.derived.iter().zip(previous).zip(next).zip(subscribed)
            {
                let changed =
                    self.compare_observed_getter(key, &getter.property.equality, subscribed, previous, next)?;
                changes.push(ObservedChange {
                    property_id: getter.property.property_id,
                    previous,
                    next,
                    changed: Some(changed),
                });
            }
            tail
        };
        self.deliver_observed_changes(head, tail, &changes)?;
        for root in derived_roots.into_iter().rev() {
            self.release_expression_root(Some(root))?;
        }
        self.builder.ins().jump(exit, &[]);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        for root in backing_roots.into_iter().rev() {
            self.release_expression_root(Some(root))?;
        }
        Some(())
    }

    /// Snapshot the tail and determine which dependent getters have live subscriptions.
    fn observed_subscriptions(&mut self, head: Value, getters: &[ObservedDerived]) -> (Value, Vec<Value>) {
        let pointer = self.frontend_config.pointer_type();
        let visit = self.builder.create_block();
        let done = self.builder.create_block();
        for block in [visit, done] {
            self.builder.append_block_param(block, pointer);
            for _ in getters {
                self.builder.append_block_param(block, types::I8);
            }
        }
        let clear = self.builder.ins().iconst(types::I8, 0);
        let initial =
            std::iter::once(head).chain(getters.iter().map(|_| clear)).map(BlockArg::Value).collect::<Vec<_>>();
        self.builder.ins().jump(visit, &initial);
        self.builder.switch_to_block(visit);
        let values = self.builder.block_params(visit).to_vec();
        let node = values[0];
        let key = self.builder.ins().load(pointer, MemFlagsData::new(), node, OBSERVER_NODE_PROPERTY_OFFSET);
        let next = self.builder.ins().load(pointer, MemFlagsData::new(), node, OBSERVER_NODE_NEXT_OFFSET);
        let mut flags = Vec::with_capacity(values.len());
        flags.push(BlockArg::Value(next));
        for (getter, &seen) in getters.iter().zip(&values[1..]) {
            let matched = self.builder.ins().icmp_imm_s(IntCC::Equal, key, getter.property.property_id as i64);
            flags.push(BlockArg::Value(self.builder.ins().bor(matched, seen)));
        }
        let mut result = flags.clone();
        result[0] = BlockArg::Value(node);
        self.builder.ins().brif(next, visit, &flags, done, &result);
        self.builder.seal_block(visit);
        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        let values = self.builder.block_params(done);
        (values[0], values[1..].to_vec())
    }

    fn capture_observed_getter(
        &mut self,
        key: AstNodeKey,
        receiver: Value,
        getter: &ObservedDerived,
        subscribed: Value,
    ) -> Option<Value> {
        let [returned] = getter.signature.returns.as_slice() else { return None };
        let ty = returned.value_type;
        // The inactive arm's payload cannot be delivered. A zero placeholder also gives
        // a managed result a null root on that arm.
        let unused = match ty {
            types::F32 => self.builder.ins().f32const(0.0),
            types::F64 => self.builder.ins().f64const(0.0),
            _ if ty.is_int() => self.builder.ins().iconst(ty, 0),
            _ => return None,
        };
        let evaluate = self.builder.create_block();
        let done = self.builder.create_block();
        let value = self.builder.append_block_param(done, ty);
        self.builder.ins().brif(subscribed, evaluate, &[], done, &[BlockArg::Value(unused)]);
        self.builder.switch_to_block(evaluate);
        self.builder.seal_block(evaluate);
        let call = self.invoke_accessor(key, getter.getter.clone(), getter.signature.clone(), &[receiver])?;
        let &[next] = self.builder.inst_results(call) else { return None };
        self.builder.ins().jump(done, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        Some(value)
    }

    fn compare_observed_getter(
        &mut self,
        key: AstNodeKey,
        equality: &ObservationEquality,
        subscribed: Value,
        previous: Value,
        next: Value,
    ) -> Option<Value> {
        let compare = self.builder.create_block();
        let done = self.builder.create_block();
        let changed = self.builder.append_block_param(done, types::I8);
        let clear = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().brif(subscribed, compare, &[], done, &[BlockArg::Value(clear)]);
        self.builder.switch_to_block(compare);
        self.builder.seal_block(compare);
        let equal = self.observation_equal(key, equality, previous, next)?;
        let different = self.builder.ins().icmp_imm_s(IntCC::Equal, equal, 0);
        self.builder.ins().jump(done, &[BlockArg::Value(different)]);
        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        Some(changed)
    }

    fn observation_equal(
        &mut self,
        key: AstNodeKey,
        equality: &ObservationEquality,
        previous: Value,
        next: Value,
    ) -> Option<Value> {
        let ty = self.builder.func.dfg.value_type(previous);
        (self.builder.func.dfg.value_type(next) == ty).then_some(())?;
        Some(match equality {
            ObservationEquality::Method(callee, signature) => {
                let call = self.invoke_accessor(key, callee.clone(), signature.clone(), &[previous, next])?;
                let [equal] = self.builder.inst_results(call) else { return None };
                *equal
            }
            ObservationEquality::Integer if ty.is_int() => self.builder.ins().icmp(IntCC::Equal, previous, next),
            ObservationEquality::Float if ty.is_float() => self.builder.ins().fcmp(FloatCC::Equal, previous, next),
            ObservationEquality::String if ty == self.frontend_config.pointer_type() => {
                self.emit_corelib_service_call(key, "str_eq", &[previous, next], &[ty, ty], Some(types::I64))?
            }
            _ => return None,
        })
    }

    fn deliver_observed_changes(&mut self, head: Value, tail: Value, changes: &[ObservedChange]) -> Option<()> {
        let pointer = self.frontend_config.pointer_type();
        let exit = self.builder.create_block();
        self.builder.declare_value_needs_stack_map(tail);
        let visit = self.builder.create_block();
        let node = self.builder.append_block_param(visit, pointer);
        self.builder.declare_value_needs_stack_map(node);
        self.builder.ins().jump(visit, &[BlockArg::Value(head)]);

        self.builder.switch_to_block(visit);
        self.builder.ins().trapz(node, TrapCode::unwrap_user(10));
        let property = self.builder.ins().load(pointer, MemFlagsData::new(), node, OBSERVER_NODE_PROPERTY_OFFSET);
        let advance = self.builder.create_block();
        for change in changes {
            let matches = self.builder.ins().icmp_imm_s(IntCC::Equal, property, change.property_id as i64);
            let notify =
                if let Some(changed) = change.changed { self.builder.ins().band(matches, changed) } else { matches };
            let deliver = self.builder.create_block();
            let other = self.builder.create_block();
            self.builder.ins().brif(notify, deliver, &[], other, &[]);

            self.builder.switch_to_block(deliver);
            self.builder.seal_block(deliver);
            let callback = self.builder.ins().load(pointer, MemFlagsData::new(), node, OBSERVER_NODE_CLOSURE_OFFSET);
            let callback_root = self.root_expression_value(callback)?;
            let ty = self.builder.func.dfg.value_type(change.previous);
            let _result =
                self.call_closure_record(callback, (16, 24), &[ty, ty], &[change.previous, change.next], None);
            self.guard_checked_allocation()?;
            self.release_expression_root(Some(callback_root))?;
            self.builder.ins().jump(advance, &[]);
            self.builder.switch_to_block(other);
            self.builder.seal_block(other);
        }
        self.builder.ins().jump(advance, &[]);

        self.builder.switch_to_block(advance);
        self.builder.seal_block(advance);
        // Reentrant registrations may append to the list. A single write delivers to the
        // snapshot taken before its first callback, even if the last callback appends a node.
        let done = self.builder.ins().icmp(IntCC::Equal, node, tail);
        let next = self.builder.ins().load(pointer, MemFlagsData::new(), node, OBSERVER_NODE_NEXT_OFFSET);
        self.builder.ins().brif(done, exit, &[], visit, &[BlockArg::Value(next)]);
        self.builder.seal_block(visit);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }
}
