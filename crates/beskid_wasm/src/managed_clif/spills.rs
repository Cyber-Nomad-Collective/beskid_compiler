//! The native lowering can reuse one spill slot for values with disjoint lifetimes.

#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};

use cranelift_codegen::{
    flowgraph::ControlFlowGraph,
    ir::{Block, Function, Inst, InstructionData, Opcode, StackSlot, Value, ValueDef, types::I64},
};

pub(super) fn values_at_loads(function: &Function, cfg: &ControlFlowGraph) -> HashMap<Inst, Value> {
    let slotOf = |address| {
        let ValueDef::Result(inst, _) = function.dfg.value_def(function.dfg.resolve_aliases(address)) else {
            return None;
        };
        let InstructionData::StackAddr { stack_slot, offset, .. } = function.dfg.insts[inst] else {
            return None;
        };
        (i32::from(offset) == 0).then_some(stack_slot)
    };
    let mut lastStores = HashMap::new();
    let mut values = HashMap::new();
    let mut unresolved = Vec::new();
    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            match function.dfg.insts[inst] {
                InstructionData::Store { opcode: Opcode::Store, args: [value, address], offset, .. }
                    if i32::from(offset) == 0 && function.dfg.value_type(value) == I64 =>
                {
                    if let Some(slot) = slotOf(address) {
                        lastStores.insert((block, slot), function.dfg.resolve_aliases(value));
                    }
                }
                InstructionData::Load { opcode: Opcode::Load, arg, offset, .. }
                    if i32::from(offset) == 0 && function.dfg.value_type(function.dfg.first_result(inst)) == I64 =>
                {
                    if let Some(slot) = slotOf(arg) {
                        if let Some(&value) = lastStores.get(&(block, slot)) {
                            values.insert(inst, value);
                        } else {
                            unresolved.push((inst, block, slot));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let entry = function.layout.entry_block().expect("verified function has an entry");
    let mut cache = HashMap::new();
    for (inst, block, slot) in unresolved {
        let source = cache
            .entry((block, slot))
            .or_insert_with(|| unique_predecessor_value(block, entry, slot, cfg, &lastStores));
        if let Some(value) = source {
            values.insert(inst, *value);
        }
    }
    values
}

fn unique_predecessor_value(
    block: Block,
    entry: Block,
    slot: StackSlot,
    cfg: &ControlFlowGraph,
    lastStores: &HashMap<(Block, StackSlot), Value>,
) -> Option<Value> {
    if block == entry {
        return None;
    }
    let mut visited = HashSet::new();
    let mut pending: Vec<_> = cfg.pred_iter(block).map(|predecessor| predecessor.block).collect();
    let mut source = None;
    while let Some(predecessor) = pending.pop() {
        if !visited.insert(predecessor) {
            continue;
        }
        if let Some(&value) = lastStores.get(&(predecessor, slot)) {
            if source.is_some_and(|previous| previous != value) {
                return None;
            }
            source = Some(value);
        } else if predecessor == entry {
            return None;
        } else {
            pending.extend(cfg.pred_iter(predecessor).map(|parent| parent.block));
        }
    }
    source
}
