//! Every opcode admitted in a `clif { ... }` block must have instruction selection on both
//! supported 64-bit targets. A verified block that Cranelift cannot lower would otherwise pass
//! type checking and fail only at JIT or AOT compile time (C53-3).
//!
//! Each allowlisted opcode has one representative function below; the test compiles it for
//! x86-64 (x86-64-v2 features) and aarch64 without running it. Opcodes that import emulates
//! (`*_overflow_cin`, `*_overflow_bin`) are represented by their emulated sequence.

use beskid_analysis::clif_surface::CLIF_ALLOWED_OPCODES;
use cranelift_codegen::isa::{self, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use std::sync::Arc;

/// `(opcode, function text)`.
const REPRESENTATIVES: &[(&str, &str)] = &[
    ("iconst", "function %f() -> i64 {\nblock0:\n    v0 = iconst.i64 7\n    return v0\n}"),
    ("f32const", "function %f() -> f32 {\nblock0:\n    v0 = f32const 0x1.0p0\n    return v0\n}"),
    ("f64const", "function %f() -> f64 {\nblock0:\n    v0 = f64const 0x1.0p0\n    return v0\n}"),
    ("iadd", BINARY_I64),
    ("isub", BINARY_I64),
    ("ineg", UNARY_I64),
    ("iabs", UNARY_I64),
    ("imul", BINARY_I64),
    ("umulhi", BINARY_I64),
    ("smulhi", BINARY_I64),
    ("udiv", BINARY_I64),
    ("sdiv", BINARY_I64),
    ("urem", BINARY_I64),
    ("srem", BINARY_I64),
    ("smin", BINARY_I64),
    ("umin", BINARY_I64),
    ("smax", BINARY_I64),
    ("umax", BINARY_I64),
    ("uadd_sat", BINARY_I16X8),
    ("sadd_sat", BINARY_I16X8),
    ("usub_sat", BINARY_I16X8),
    ("ssub_sat", BINARY_I16X8),
    ("uadd_overflow", OVERFLOW_I64),
    ("sadd_overflow", OVERFLOW_I64),
    ("usub_overflow", OVERFLOW_I64),
    ("ssub_overflow", OVERFLOW_I64),
    ("umul_overflow", OVERFLOW_I64),
    ("smul_overflow", OVERFLOW_I64),
    ("uadd_overflow_cin", "EMULATED uadd_overflow bor"),
    ("sadd_overflow_cin", "EMULATED sadd_overflow bxor"),
    ("usub_overflow_bin", "EMULATED usub_overflow bor"),
    ("ssub_overflow_bin", "EMULATED ssub_overflow bxor"),
    ("band", BINARY_I64),
    ("bor", BINARY_I64),
    ("bxor", BINARY_I64),
    ("bnot", UNARY_I64),
    ("ishl", BINARY_I64),
    ("ushr", BINARY_I64),
    ("sshr", BINARY_I64),
    ("rotl", BINARY_I64),
    ("rotr", BINARY_I64),
    ("clz", UNARY_I64),
    ("cls", UNARY_I64),
    ("ctz", UNARY_I64),
    ("popcnt", UNARY_I64),
    ("bswap", UNARY_I64),
    ("bitrev", UNARY_I64),
    (
        "icmp",
        "function %f(i64, i64) -> i8 {\nblock0(v0: i64, v1: i64):\n    v2 = icmp ult v0, v1\n    return v2\n}",
    ),
    (
        "select",
        "function %f(i8, i64, i64) -> i64 {\nblock0(v0: i8, v1: i64, v2: i64):\n    v3 = select v0, v1, v2\n    \
         return v3\n}",
    ),
    (
        "bitselect",
        "function %f(i64, i64, i64) -> i64 {\nblock0(v0: i64, v1: i64, v2: i64):\n    v3 = bitselect v0, v1, \
         v2\n    return v3\n}",
    ),
    ("bmask", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = bmask.i64 v0\n    return v1\n}"),
    ("uextend", "function %f(i32) -> i64 {\nblock0(v0: i32):\n    v1 = uextend.i64 v0\n    return v1\n}"),
    ("sextend", "function %f(i32) -> i64 {\nblock0(v0: i32):\n    v1 = sextend.i64 v0\n    return v1\n}"),
    ("ireduce", "function %f(i64) -> i32 {\nblock0(v0: i64):\n    v1 = ireduce.i32 v0\n    return v1\n}"),
    ("bitcast", "function %f(i64) -> f64 {\nblock0(v0: i64):\n    v1 = bitcast.f64 v0\n    return v1\n}"),
    ("fadd", BINARY_F64),
    ("fsub", BINARY_F64),
    ("fmul", BINARY_F64),
    ("fdiv", BINARY_F64),
    ("fneg", UNARY_F64),
    ("fabs", UNARY_F64),
    ("fmin", BINARY_F64),
    ("fmax", BINARY_F64),
    ("sqrt", UNARY_F64),
    (
        "fcmp",
        "function %f(f64, f64) -> i8 {\nblock0(v0: f64, v1: f64):\n    v2 = fcmp lt v0, v1\n    return v2\n}",
    ),
    (
        "fcvt_from_sint",
        "function %f(i64) -> f64 {\nblock0(v0: i64):\n    v1 = fcvt_from_sint.f64 v0\n    return v1\n}",
    ),
    (
        "fcvt_from_uint",
        "function %f(i64) -> f64 {\nblock0(v0: i64):\n    v1 = fcvt_from_uint.f64 v0\n    return v1\n}",
    ),
    (
        "fcvt_to_sint_sat",
        "function %f(f64) -> i64 {\nblock0(v0: f64):\n    v1 = fcvt_to_sint_sat.i64 v0\n    return v1\n}",
    ),
    (
        "fcvt_to_uint_sat",
        "function %f(f64) -> i64 {\nblock0(v0: f64):\n    v1 = fcvt_to_uint_sat.i64 v0\n    return v1\n}",
    ),
    ("splat", "function %f(i32) -> i32x4 {\nblock0(v0: i32):\n    v1 = splat.i32x4 v0\n    return v1\n}"),
    (
        "insertlane",
        "function %f(i32x4, i32) -> i32x4 {\nblock0(v0: i32x4, v1: i32):\n    v2 = insertlane v0, v1, 1\n    \
         return v2\n}",
    ),
    (
        "extractlane",
        "function %f(i32x4) -> i32 {\nblock0(v0: i32x4):\n    v1 = extractlane v0, 2\n    return v1\n}",
    ),
    (
        "shuffle",
        "function %f(i8x16, i8x16) -> i8x16 {\nblock0(v0: i8x16, v1: i8x16):\n    v2 = shuffle v0, v1, \
         0x1f1e1d1c1b1a19180706050403020100\n    return v2\n}",
    ),
    (
        "swizzle",
        "function %f(i8x16, i8x16) -> i8x16 {\nblock0(v0: i8x16, v1: i8x16):\n    v2 = swizzle v0, v1\n    \
         return v2\n}",
    ),
    ("vany_true", "function %f(i32x4) -> i8 {\nblock0(v0: i32x4):\n    v1 = vany_true v0\n    return v1\n}"),
    ("vall_true", "function %f(i32x4) -> i8 {\nblock0(v0: i32x4):\n    v1 = vall_true v0\n    return v1\n}"),
    (
        "vhigh_bits",
        "function %f(i32x4) -> i32 {\nblock0(v0: i32x4):\n    v1 = vhigh_bits.i32 v0\n    return v1\n}",
    ),
    ("snarrow", NARROW_I32X4),
    ("unarrow", NARROW_I32X4),
    ("swiden_low", WIDEN_I16X8),
    ("swiden_high", WIDEN_I16X8),
    ("uwiden_low", WIDEN_I16X8),
    ("uwiden_high", WIDEN_I16X8),
    (
        "iadd_pairwise",
        "function %f(i32x4, i32x4) -> i32x4 {\nblock0(v0: i32x4, v1: i32x4):\n    v2 = iadd_pairwise v0, v1\n    \
         return v2\n}",
    ),
    ("trapz", "function %f(i64) {\nblock0(v0: i64):\n    trapz v0, user1\n    return\n}"),
    ("trapnz", "function %f(i64) {\nblock0(v0: i64):\n    trapnz v0, user1\n    return\n}"),
    ("load", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = load.i64 v0+8\n    return v1\n}"),
    ("uload8", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = uload8.i64 v0\n    return v1\n}"),
    ("sload8", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = sload8.i64 v0\n    return v1\n}"),
    ("uload16", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = uload16.i64 v0\n    return v1\n}"),
    ("sload16", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = sload16.i64 v0\n    return v1\n}"),
    ("uload32", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = uload32 v0\n    return v1\n}"),
    ("sload32", "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = sload32 v0\n    return v1\n}"),
    ("store", "function %f(i64, i64) {\nblock0(v0: i64, v1: i64):\n    store v1, v0+8\n    return\n}"),
    ("istore8", "function %f(i64, i64) {\nblock0(v0: i64, v1: i64):\n    istore8 v1, v0\n    return\n}"),
    ("istore16", "function %f(i64, i64) {\nblock0(v0: i64, v1: i64):\n    istore16 v1, v0\n    return\n}"),
    ("istore32", "function %f(i64, i64) {\nblock0(v0: i64, v1: i64):\n    istore32 v1, v0\n    return\n}"),
];

const UNARY_I64: &str = "function %f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = OP v0\n    return v1\n}";
const BINARY_I64: &str =
    "function %f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n    v2 = OP v0, v1\n    return v2\n}";
const OVERFLOW_I64: &str =
    "function %f(i64, i64) -> i64, i8 {\nblock0(v0: i64, v1: i64):\n    v2, v3 = OP v0, v1\n    return v2, v3\n}";
const BINARY_I16X8: &str =
    "function %f(i16x8, i16x8) -> i16x8 {\nblock0(v0: i16x8, v1: i16x8):\n    v2 = OP v0, v1\n    return v2\n}";
const UNARY_F64: &str = "function %f(f64) -> f64 {\nblock0(v0: f64):\n    v1 = OP v0\n    return v1\n}";
const BINARY_F64: &str =
    "function %f(f64, f64) -> f64 {\nblock0(v0: f64, v1: f64):\n    v2 = OP v0, v1\n    return v2\n}";
const NARROW_I32X4: &str =
    "function %f(i32x4, i32x4) -> i16x8 {\nblock0(v0: i32x4, v1: i32x4):\n    v2 = OP v0, v1\n    return v2\n}";
const WIDEN_I16X8: &str = "function %f(i16x8) -> i32x4 {\nblock0(v0: i16x8):\n    v1 = OP v0\n    return v1\n}";

/// The two-step sequence `clif_block` import emits for a carry-in or borrow-in opcode.
fn emulated(step: &str, combine: &str) -> String {
    format!(
        "function %f(i64, i64, i8) -> i64, i8 {{\nblock0(v0: i64, v1: i64, v2: i8):\n    v10 = iconst.i8 0\n    \
         v3 = icmp ne v2, v10\n    v4 = uextend.i64 v3\n    v5, v6 = {step} v0, v1\n    v7, v8 = {step} v5, v4\n    v9 = {combine} v6, v8\n    \
         return v7, v9\n}}"
    )
}

fn representative(opcode: &str) -> Option<String> {
    let (_, text) = REPRESENTATIVES.iter().find(|(name, _)| *name == opcode)?;
    Some(match text.strip_prefix("EMULATED ") {
        Some(rest) => {
            let (step, combine) = rest.split_once(' ').expect("emulated representative names step and combine");
            emulated(step, combine)
        }
        None => text.replace("OP", opcode),
    })
}

fn target_isa(triple: &str) -> Arc<dyn TargetIsa> {
    let mut builder = isa::lookup(triple.parse().expect("valid triple")).expect("target compiled in");
    if triple.starts_with("x86_64") {
        for feature in ["has_sse3", "has_ssse3", "has_sse41", "has_sse42", "has_popcnt"] {
            builder.enable(feature).expect("x86-64-v2 feature");
        }
    }
    let mut shared = settings::builder();
    shared.set("opt_level", "speed").expect("opt_level");
    builder.finish(settings::Flags::new(shared)).expect("target ISA")
}

#[test]
fn every_allowlisted_clif_opcode_has_a_representative() {
    let missing =
        CLIF_ALLOWED_OPCODES.iter().filter(|opcode| representative(opcode).is_none()).collect::<Vec<_>>();
    assert!(missing.is_empty(), "add a lowering representative for {missing:?}");
}

#[test]
fn every_allowlisted_clif_opcode_lowers_on_x86_64_and_aarch64() {
    let mut failures = Vec::new();
    for triple in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        let isa = target_isa(triple);
        for opcode in CLIF_ALLOWED_OPCODES {
            let Some(text) = representative(opcode) else { continue };
            let function = match cranelift_reader::parse_functions(&text) {
                Ok(mut functions) => functions.remove(0),
                Err(error) => {
                    failures.push(format!("{triple} {opcode}: representative does not parse: {error}"));
                    continue;
                }
            };
            let mut context = cranelift_codegen::Context::for_function(function);
            if let Err(error) = context.compile(isa.as_ref(), &mut Default::default()) {
                failures.push(format!("{triple} {opcode}: {:?}", error.inner));
            }
        }
    }
    assert!(failures.is_empty(), "allowlisted opcodes without lowering:\n{}", failures.join("\n"));
}
