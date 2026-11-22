//! Adapt the managed receiver of a contract box. The immutable witness tables,
//! relocations and their method entry points come from the shared compiler.
use super::*;
use beskid_codegen::contract_static::WitnessTablePlan;
use beskid_queries::{CONTRACT_BOX_RECEIVER_OFFSET, CONTRACT_BOX_WITNESS_OFFSET, WITNESS_HEADER_WORDS};

pub(super) struct Dispatch<'a> {
    pub record: Option<usize>,
    pub tables: &'a [WitnessTablePlan],
    pub results: Option<&'a HashMap<Inst, Option<ReferenceKind>>>,
    pub pointers: HashSet<Value>,
    pub slots: HashMap<Value, usize>,
    pub receivers: HashMap<Value, Value>,
}

impl Pass<'_> {
    pub(super) fn store_contract(
        &mut self,
        inst: Inst,
        handle: Value,
        record: usize,
        offset: i64,
        value: Value,
    ) -> Result<Edit, &'static str> {
        if offset == i64::from(CONTRACT_BOX_RECEIVER_OFFSET)
            && let Some(&original) = self.dispatch.receivers.get(&value)
        {
            // An embedded-contract conversion preserves the original receiver and its GC root.
            for (field, ty) in [(0, I32), (1, I64)] {
                let getter = self.callBefore(inst, &RecordOp::Get(field).symbol(record), &[original], Some(ty));
                let input = self.function.dfg.first_result(getter);
                self.callBefore(inst, &RecordOp::Set(field).symbol(record), &[handle, input], None);
            }
            return Ok(Edit::Erase);
        }
        let (field, input) = match offset {
            n if n == i64::from(CONTRACT_BOX_WITNESS_OFFSET) && self.dispatch.pointers.contains(&value) => (2, value),
            n if n == i64::from(CONTRACT_BOX_RECEIVER_OFFSET) => match self.source(value) {
                Some(reference) => (0, ReferenceKind::of(reference).ok_or("invalid contract receiver")?.1),
                None => (1, value),
            },
            _ => return Err("invalid contract box initialization"),
        };
        self.callBefore(inst, &RecordOp::Set(field).symbol(record), &[handle, input], None);
        Ok(Edit::Erase)
    }

    pub(super) fn load_contract(
        &mut self,
        inst: Inst,
        handle: Value,
        record: usize,
        offset: i32,
    ) -> Result<Edit, &'static str> {
        if offset == CONTRACT_BOX_WITNESS_OFFSET as i32 {
            self.dispatch.pointers.insert(self.function.dfg.first_result(inst));
            return self.replaceCall(inst, &RecordOp::Get(2).symbol(record), &[handle], I64);
        }
        if offset != CONTRACT_BOX_RECEIVER_OFFSET as i32 {
            return Err("invalid contract receiver offset");
        }
        let reference = self.callBefore(inst, &RecordOp::Get(0).symbol(record), &[handle], Some(I32));
        let reference = self.function.dfg.first_result(reference);
        let scalar = self.callBefore(inst, &RecordOp::Get(1).symbol(record), &[handle], Some(I64));
        let scalar = self.function.dfg.first_result(scalar);
        let word = self.rewindBefore(inst).ins().uextend(I64, reference);
        self.dispatch.receivers.insert(self.function.dfg.first_result(inst), handle);
        self.function.replace(inst).select(reference, word, scalar);
        Ok(Edit::Replace)
    }

    pub(super) fn load_witness(&mut self, inst: Inst, offset: i32) -> Result<Edit, &'static str> {
        let word = usize::try_from(offset).ok().filter(|_| offset % 8 == 0).ok_or("invalid witness table offset")? / 8;
        if word >= WITNESS_HEADER_WORDS as usize {
            let slot = word - WITNESS_HEADER_WORDS as usize;
            let value = self.function.dfg.first_result(inst);
            self.dispatch.slots.insert(value, slot);
            if self
                .dispatch
                .tables
                .iter()
                .any(|table| slot >= table.entries.len() && slot < table.entries.len() + table.embedded.len())
            {
                self.dispatch.pointers.insert(value);
            }
        }
        Ok(Edit::Keep)
    }

    pub(super) fn dispatch_contract(
        &mut self,
        inst: Inst,
        slot: usize,
        sig: cranelift_codegen::ir::SigRef,
        callee: Value,
        args: &[Value],
    ) -> Result<Edit, &'static str> {
        if !self.dispatch.tables.iter().any(|table| table.entries.get(slot).is_some()) {
            return Err("contract method slot has no implementation");
        }
        let result = self.dispatch.results.and_then(|calls| calls.get(&inst)).copied()
            .ok_or("contract call has no source result type")?;
        let mut translated = Vec::new();
        for &arg in args {
            let value = match self.source(arg) {
                Some(source) => {
                    let (_, handle) = ReferenceKind::of(source).ok_or("invalid contract call argument")?;
                    self.rewindBefore(inst).ins().uextend(I64, handle)
                }
                None => arg,
            };
            translated.push(value);
        }
        let call = self.rewindBefore(inst).ins().call_indirect(sig, callee, &translated);
        if let Some(kind) = result {
            let word = self.function.dfg.first_result(call);
            let reference = self.rewindBefore(inst).ins().ireduce(I32, word);
            return self.eraseResult(inst, kind.bind(reference));
        }
        self.function.dfg.replace_with_aliases(inst, call);
        Ok(Edit::Erase)
    }
}

pub(super) fn retain_witness_globals(function: &mut Function) {
    let original = std::mem::take(&mut function.global_values);
    let mut remap = HashMap::new();
    let mut references = Vec::new();
    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            if let InstructionData::UnaryGlobalValue { global_value, .. } = function.dfg.insts[inst] {
                references.push((inst, global_value));
            }
        }
    }
    for (inst, old) in references {
        let new = *remap.entry(old).or_insert_with(|| function.global_values.push(original[old].clone()));
        if let InstructionData::UnaryGlobalValue { global_value, .. } = &mut function.dfg.insts[inst] {
            *global_value = new;
        }
    }
}
