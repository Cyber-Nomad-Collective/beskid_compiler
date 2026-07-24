//! Typed load/store helpers for struct and scalar field memory.

use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_frontend::FunctionBuilder;

/// Narrow or widen `value` to match `target` for integer CLIF types.
pub(crate) fn coerce_value_to_clif_type(builder: &mut FunctionBuilder, value: Value, target: Type) -> Value {
    let value_ty = builder.func.dfg.value_type(value);
    if value_ty == target {
        return value;
    }
    if value_ty.is_int() && target.is_int() {
        if value_ty.bits() < target.bits() {
            return builder.ins().sextend(target, value);
        }
        if value_ty.bits() > target.bits() {
            return builder.ins().ireduce(target, value);
        }
    }
    value
}

/// Store `value` at `addr` using the width of `clif_ty` (avoids corrupting adjacent fields).
pub(crate) fn store_typed_value(
    builder: &mut FunctionBuilder,
    clif_ty: Type,
    value: Value,
    addr: Value,
    flags: MemFlags,
) {
    let coerced = coerce_value_to_clif_type(builder, value, clif_ty);
    builder.ins().store(flags, coerced, addr, 0);
}
