//! Turn compiler-generated ABI owner slots into scheduler-held Wasm GC references or scalar bits.

use super::*;
use crate::scheduler::{ABI_CLEAR, ABI_INITIALIZE, Transfer};

pub(super) fn fiber_join_layouts(
    artifact: &CodegenArtifact,
    project: impl Fn(&SourceSignatureType) -> Result<Option<ReferenceKind>, Box<dyn Error>>,
    existing: &mut HashMap<SourceSignatureType, usize>,
    records: &mut Vec<Vec<Field>>,
    offsets: &mut Vec<Vec<i64>>,
    arrays: &mut std::collections::BTreeSet<ArrayElement>,
) -> Result<HashMap<String, usize>, Box<dyn Error>> {
    let mut joins = HashMap::new();
    for lowered in &artifact.functions {
        let joins_fiber = lowered.function.dfg.ext_funcs.values().any(|external| {
            matches!(&external.name, ExternalName::TestCase(name) if name.raw() == crate::fibers::JOIN_VALUE.as_bytes())
        });
        if !joins_fiber {
            continue;
        }
        let signature = artifact.source_signatures.get(&lowered.name).ok_or("fiber join lacks its source signature")?;
        let [SourceSignatureType::Nominal(receiver)] = signature.parameters.as_ref() else {
            return Err("fiber join requires a specialized receiver".into());
        };
        let [result] = receiver.arguments.as_ref() else {
            return Err("fiber join receiver lacks its result type".into());
        };
        let record = if let Some(&record) = existing.get(result) {
            record
        } else {
            let field = match project(result)? {
                Some(ReferenceKind::String) => Field::String,
                Some(ReferenceKind::Record(record)) => Field::Record(record),
                Some(ReferenceKind::Array(element)) => {
                    arrays.insert(element);
                    Field::Array(element)
                }
                None => {
                    let SourceSignatureType::Scalar(ty) = result else {
                        return Err("fiber result requires a concrete scalar type".into());
                    };
                    let kind = if *ty == SemanticTypeId::UNIT { Scalar::I64 }
                        else { scalar(*ty).ok_or("fiber result has no scalar representation")? };
                    Field::Scalar(kind)
                }
                Some(ReferenceKind::Environment) => return Err("fiber result cannot be an untyped environment".into()),
            };
            let offset = artifact.browser_fiber_payload_offset.ok_or("fiber result ABI lacks its value offset")?;
            let record = records.len();
            records.push(vec![field]);
            offsets.push(vec![i64::try_from(offset)?]);
            existing.insert(result.clone(), record);
            record
        };
        joins.insert(lowered.name.clone(), record);
    }
    Ok(joins)
}

#[derive(Default)]
pub(super) struct Slots {
    send: HashMap<(StackSlot, Block), (Value, bool)>,
    receive: HashMap<(StackSlot, Block), Value>,
}

impl Pass<'_> {
    pub(super) fn transfer_call(
        &mut self,
        inst: Inst,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Edit, &'static str>> {
        Some(match (name, args) {
            (ABI_INITIALIZE, [slot, tag, boxed, descriptor]) => {
                self.initialize_send(inst, *slot, *tag, *boxed, *descriptor)
            }
            (ABI_CLEAR, [slot]) => self.clear_transfer(inst, *slot),
            (_, [handle, slot]) if matches!(crate::scheduler::find(name)?.transfer, Transfer::Receive) => {
                self.receive(inst, name, *handle, *slot)
            }
            (_, [handle, slot, extra @ ..]) if matches!(crate::scheduler::find(name)?.transfer, Transfer::Send) => {
                self.send(inst, name, *handle, *slot, extra)
            }
            _ => return None,
        })
    }

    fn transfer_slot(&self, inst: Inst, address: Value) -> Result<(StackSlot, Block), &'static str> {
        let Some(Native::FiberSlot(slot, 0)) = self.source(address) else {
            return Err("typed scheduler value requires a compiler-generated owner slot");
        };
        let block = self.function.layout.inst_block(inst).ok_or("scheduler transfer has no block")?;
        Ok((slot, block))
    }

    fn initialize_send(
        &mut self,
        inst: Inst,
        slot: Value,
        tag: Value,
        boxed: Value,
        descriptor: Value,
    ) -> Result<Edit, &'static str> {
        let key = self.transfer_slot(inst, slot)?;
        let Some(Native::Packet(box_ref, record)) = self.source(boxed) else {
            return Err("scheduler sender has no proven managed payload box");
        };
        if self.constant(tag) != Some(1) || self.source(descriptor) != Some(Native::TransferDescriptor(record)) {
            return Err("scheduler sender has an invalid payload descriptor");
        }
        let [field] = self.fields[record].as_slice() else {
            return Err("scheduler payload box must contain exactly one field");
        };
        let field = *field;
        let (ty, managed) = match field {
            Field::Scalar(scalar) => (scalar_type(scalar), false),
            Field::String | Field::Array(_) | Field::Record(_) => (I32, true),
            Field::AnyRecord => return Err("scheduler payload box lacks a source field kind"),
        };
        let getter = self.callBefore(inst, &RecordOp::Get(0).symbol(record), &[box_ref], Some(ty));
        let value = self.function.dfg.first_result(getter);
        let word = self.pack_transfer_bits(inst, value, ty);
        if self.transferSlots.send.insert(key, (word, managed)).is_some() {
            return Err("scheduler sender owner slot has already been initialized");
        }
        self.function.replace(inst).iconst(I8, 1);
        Ok(Edit::Replace)
    }

    fn send(
        &mut self,
        inst: Inst,
        name: &str,
        handle: Value,
        slot: Value,
        extra: &[Value],
    ) -> Result<Edit, &'static str> {
        let key = self.transfer_slot(inst, slot)?;
        let (bits, managed) =
            *self.transferSlots.send.get(&key).ok_or("scheduler sender owner slot is uninitialized")?;
        let flag = self.rewindBefore(inst).ins().iconst(I64, i64::from(managed));
        let mut args = vec![handle, bits, flag];
        args.extend_from_slice(extra);
        self.replaceCall(inst, name, &args, I64)
    }

    fn receive(&mut self, inst: Inst, name: &str, handle: Value, slot: Value) -> Result<Edit, &'static str> {
        let key = self.transfer_slot(inst, slot)?;
        let call = self.callBefore(inst, name, &[handle], Some(I64));
        let value = self.function.dfg.first_result(call);
        if self.transferSlots.receive.insert(key, value).is_some() {
            return Err("scheduler receiver owner slot has not been cleared");
        }
        self.function.replace(inst).iconst(I8, 1);
        Ok(Edit::Replace)
    }

    fn clear_transfer(&mut self, inst: Inst, slot: Value) -> Result<Edit, &'static str> {
        let key = self.transfer_slot(inst, slot)?;
        if self.transferSlots.send.remove(&key).is_none()
            && self.transferSlots.receive.remove(&key).is_none()
            && self.joinSlots.remove(&key).is_none()
        {
            return Err("scheduler ABI slot has no live value");
        }
        self.function.replace(inst).iconst(I8, 1);
        Ok(Edit::Replace)
    }

    pub(super) fn transferred_word(&self, inst: Inst, slot: StackSlot) -> Option<Value> {
        let block = self.function.layout.inst_block(inst)?;
        self.transferSlots.receive.get(&(slot, block)).copied()
    }

    pub(super) fn load_transferred_value(
        &mut self,
        inst: Inst,
        value: Value,
        ty: cranelift_codegen::ir::Type,
    ) -> Result<Edit, &'static str> {
        let (expected, reference) = self.transferType.ok_or("typed scheduler receive lacks its source instance")?;
        if ty != expected {
            return Err("scheduler result load disagrees with its declared type");
        }
        if let Some(reference) = reference {
            let handle = self.rewindBefore(inst).ins().ireduce(I32, value);
            return self.eraseResult(inst, reference.bind(handle));
        }
        if ty.is_float() {
            let bits = if ty.bits() == 32 { self.rewindBefore(inst).ins().ireduce(I32, value) } else { value };
            self.function.replace(inst).bitcast(ty, cranelift_codegen::ir::MemFlagsData::new(), bits);
            Ok(Edit::Replace)
        } else if ty != I64 {
            self.function.replace(inst).ireduce(ty, value);
            Ok(Edit::Replace)
        } else {
            let result = self.function.dfg.first_result(inst);
            self.function.dfg.clear_results(inst);
            self.function.dfg.change_to_alias(result, value);
            Ok(Edit::Erase)
        }
    }

    fn pack_transfer_bits(&mut self, inst: Inst, value: Value, ty: cranelift_codegen::ir::Type) -> Value {
        let mut cursor = self.rewindBefore(inst);
        let bits = if ty.is_float() {
            let integer = if ty.bits() == 32 { I32 } else { I64 };
            cursor.ins().bitcast(integer, cranelift_codegen::ir::MemFlagsData::new(), value)
        } else {
            value
        };
        if ty.bits() < 64 { cursor.ins().uextend(I64, bits) } else { bits }
    }
}
