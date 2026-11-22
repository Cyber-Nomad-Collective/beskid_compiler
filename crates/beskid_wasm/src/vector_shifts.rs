//! Cranelift shifts every lane by a scalar count. The bridge currently expects
//! a vector count for vector shifts, so emit the equivalent scalar lane operations.
use cranelift_codegen::{
    cursor::{Cursor, FuncCursor},
    ir::{Function, InstBuilder, InstructionData, Opcode},
};

pub(crate) fn expand(function: &mut Function) {
    let shifts: Vec<_> = function
        .layout
        .blocks()
        .flat_map(|block| function.layout.block_insts(block))
        .filter_map(|inst| {
            let InstructionData::Binary { opcode, args: [value, count] } = function.dfg.insts[inst] else {
                return None;
            };
            let ty = function.dfg.value_type(value);
            (ty.is_vector()
                && function.dfg.value_type(count).is_int()
                && matches!(opcode, Opcode::Ishl | Opcode::Ushr | Opcode::Sshr | Opcode::Rotl | Opcode::Rotr))
            .then_some((inst, opcode, ty, value, count))
        })
        .collect();
    for (inst, opcode, ty, value, count) in shifts {
        let mut cursor = FuncCursor::new(function);
        cursor.goto_inst(inst);
        let mut result = value;
        for lane in 0..ty.lane_count() {
            let part = cursor.ins().extractlane(value, lane as u8);
            let part = match opcode {
                Opcode::Ishl => cursor.ins().ishl(part, count),
                Opcode::Ushr => cursor.ins().ushr(part, count),
                Opcode::Sshr => cursor.ins().sshr(part, count),
                Opcode::Rotl => cursor.ins().rotl(part, count),
                Opcode::Rotr => cursor.ins().rotr(part, count),
                _ => unreachable!(),
            };
            result = cursor.ins().insertlane(result, part, lane as u8);
        }
        let old = cursor.func.dfg.first_result(inst);
        cursor.func.dfg.clear_results(inst);
        cursor.func.dfg.change_to_alias(old, result);
        cursor.remove_inst();
    }
}
