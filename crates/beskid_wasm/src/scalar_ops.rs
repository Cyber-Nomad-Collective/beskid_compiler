use cranelift_codegen::{
    cursor::{Cursor, FuncCursor},
    ir::{Function, InstBuilder, InstructionData, MemFlagsData, Opcode},
};

/// The bridge translates vector `bitselect` directly. Expand the scalar form
/// into bitwise instructions, keeping its per-bit and branch-free semantics.
pub(super) fn expand_bitselect(function: &mut Function) {
    let mut cursor = FuncCursor::new(function);
    while cursor.next_block().is_some() {
        while let Some(inst) = cursor.next_inst() {
            let InstructionData::Ternary { opcode: Opcode::Bitselect, args } = cursor.func.dfg.insts[inst] else {
                continue;
            };
            let ty = cursor.func.dfg.value_type(args[0]);
            if ty.is_vector() {
                continue;
            }

            cursor.use_srcloc(inst);
            let [mask, yes, no] = if ty.is_float() {
                args.map(|arg| cursor.ins().bitcast(ty.as_int(), MemFlagsData::new(), arg))
            } else {
                args
            };
            let difference = cursor.ins().bxor(yes, no);
            let selected = cursor.ins().band(difference, mask);
            if ty.is_float() {
                let bits = cursor.ins().bxor(no, selected);
                cursor.func.replace(inst).bitcast(ty, MemFlagsData::new(), bits);
            } else {
                cursor.func.replace(inst).bxor(no, selected);
            }
        }
    }
}
