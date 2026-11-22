use beskid_abi::SYM_STR_FROM_I64;
use wasm_encoder::{BlockType, ValType};

use super::{Functions, NEXT_HANDLE, StringTypes, rememberString, stringRef};

pub(super) fn emit_integer(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};

    const VALUE: u32 = 0;
    const MAGNITUDE: u32 = 1;
    const QUOTIENT: u32 = 2;
    const LENGTH: u32 = 3;
    const CURSOR: u32 = 4;
    const BYTES: u32 = 5;
    const NEGATIVE: u32 = 6;

    functions.defineWithLocals(
        Some(SYM_STR_FROM_I64),
        [I64],
        [I64],
        [I64, I64, I32, I32, stringRef(string.bytes), I32],
        |body| {
            // A wrapping subtraction followed by unsigned div/rem also handles i64::MIN.
            body.local_get(VALUE).i64_const(0).i64_lt_s().local_set(NEGATIVE);
            body.i64_const(0).local_get(VALUE).i64_sub().local_get(VALUE).local_get(NEGATIVE).select();
            body.local_tee(MAGNITUDE).i64_const(10).i64_div_u().local_set(QUOTIENT);
            body.local_get(NEGATIVE).i32_const(1).i32_add().local_set(LENGTH);

            body.block(BlockType::Empty).loop_(BlockType::Empty);
            body.local_get(QUOTIENT).i64_eqz().br_if(1);
            body.local_get(LENGTH).i32_const(1).i32_add().local_set(LENGTH);
            body.local_get(QUOTIENT).i64_const(10).i64_div_u().local_set(QUOTIENT).br(0);
            body.end().end();

            body.local_get(LENGTH).local_tee(CURSOR).array_new_default(string.bytes).local_set(BYTES);
            body.local_get(MAGNITUDE).local_set(QUOTIENT);

            body.loop_(BlockType::Empty);
            body.local_get(BYTES).local_get(CURSOR).i32_const(1).i32_sub().local_tee(CURSOR);
            body.local_get(QUOTIENT).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const(48).i32_add();
            body.array_set(string.bytes);
            body.local_get(QUOTIENT).i64_const(10).i64_div_u().local_tee(QUOTIENT).i64_eqz().i32_eqz().br_if(0);
            body.end();

            body.local_get(NEGATIVE).if_(BlockType::Empty);
            body.local_get(BYTES).i32_const(0).i32_const(45).array_set(string.bytes).end();
            body.global_get(NEXT_HANDLE).local_get(BYTES).ref_as_non_null();
            rememberString(body, string);
        },
    );
}
