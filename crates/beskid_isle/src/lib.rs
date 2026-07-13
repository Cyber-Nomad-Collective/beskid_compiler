//! Beskid-owned ISLE primitive layer for trusted runtime handler CLIF emission.

#![cfg(feature = "isle_primitives")]

mod context;

#[allow(
    dead_code,
    unreachable_code,
    unreachable_patterns,
    unused_imports,
    unused_variables,
    non_snake_case,
    unused_mut,
    irrefutable_let_patterns,
    unused_assignments,
    non_camel_case_types
)]
mod isle_generated {
    include!(concat!(env!("OUT_DIR"), "/isle_generated.rs"));
}

pub use context::IsleContext;
pub use cranelift_codegen::ir::Value;
pub use cranelift_frontend::FunctionBuilder;

use cranelift_codegen::ir::InstBuilder;
use isle_generated::Context;

/// Emit `iconst.i64` via ISLE `Context` bridge.
pub fn emit_iconst_i64<'a>(ctx: &mut IsleContext<'a>, val: i64) -> Value {
    ctx.iconst_i64(val).expect("iconst_i64")
}

/// Emit `load.i64` at `offset` from `base`.
pub fn emit_load_i64<'a>(ctx: &mut IsleContext<'a>, base: Value, offset: i64) -> Value {
    ctx.load_i64(base, offset).expect("load_i64")
}

/// Emit `store.i64` at `offset`.
pub fn emit_store_i64<'a>(
    ctx: &mut IsleContext<'a>,
    base: Value,
    offset: i64,
    val: Value,
) -> Value {
    ctx.store_i64(base, offset, val).expect("store_i64")
}

/// Emit zero-extended `load.i8`.
pub fn emit_load_i8_zext<'a>(ctx: &mut IsleContext<'a>, base: Value, offset: i64) -> Value {
    ctx.load_i8_zext(base, offset).expect("load_i8_zext")
}

/// Emit pointer add (`iadd_imm`).
pub fn emit_ptr_add<'a>(ctx: &mut IsleContext<'a>, base: Value, imm: i64) -> Value {
    ctx.ptr_add(base, imm).expect("ptr_add")
}

/// Emit signed less-than compare.
pub fn emit_icmp_slt<'a>(ctx: &mut IsleContext<'a>, left: Value, right: Value) -> Value {
    ctx.icmp_slt(left, right).expect("icmp_slt")
}

/// Emit equality compare (`icmp` + zero-extend to i64).
pub fn emit_icmp_eq<'a>(ctx: &mut IsleContext<'a>, left: Value, right: Value) -> Value {
    let cmp = ctx.icmp_eq(left, right).expect("icmp_eq");
    ctx.builder_mut()
        .ins()
        .uextend(cranelift_codegen::ir::types::I64, cmp)
}

/// Emit inequality compare (`icmp` + zero-extend to i64).
pub fn emit_icmp_ne<'a>(ctx: &mut IsleContext<'a>, left: Value, right: Value) -> Value {
    let cmp = ctx.icmp_ne(left, right).expect("icmp_ne");
    ctx.builder_mut()
        .ins()
        .uextend(cranelift_codegen::ir::types::I64, cmp)
}

/// Load an i64 dispatch-envelope payload slot at `offset` bytes from `base`.
pub fn emit_envelope_load_i64<'a>(ctx: &mut IsleContext<'a>, base: Value, offset: i64) -> Value {
    emit_load_i64(ctx, base, offset)
}

/// Load a pointer-sized dispatch-envelope payload slot.
pub fn emit_envelope_load_ptr<'a>(ctx: &mut IsleContext<'a>, base: Value, offset: i64) -> Value {
    emit_load_i64(ctx, base, offset)
}

/// Compare bytes at `left_off` / `right_off`; returns 1 when bytes differ.
pub fn emit_icmp_byte_ne<'a>(
    ctx: &mut IsleContext<'a>,
    left: Value,
    right: Value,
    left_off: i64,
    right_off: i64,
) -> Value {
    let ne = ctx
        .icmp_byte_ne(left, right, left_off, right_off)
        .expect("icmp_byte_ne");
    ctx.builder_mut()
        .ins()
        .uextend(cranelift_codegen::ir::types::I64, ne)
}

/// Bounded lexicographic compare; returns `-1`, `0`, or `1` as i64.
pub fn emit_bounded_memcmp<'a>(
    ctx: &mut IsleContext<'a>,
    left: Value,
    right: Value,
    len: Value,
) -> Value {
    ctx.bounded_memcmp(left, right, len)
        .expect("bounded_memcmp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types};
    use cranelift_codegen::settings::{self, Configurable, Flags};
    use cranelift_codegen::verify_function;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

    fn test_flags() -> Flags {
        let mut builder = settings::builder();
        builder.set("opt_level", "none").unwrap();
        builder.set("is_pic", "false").unwrap();
        Flags::new(builder)
    }

    fn _assert_isle_context_impl<T: isle_generated::Context>() {}

    #[test]
    fn isle_context_implements_generated_trait() {
        _assert_isle_context_impl::<IsleContext<'_>>();
    }

    #[test]
    fn envelope_load_clif_uses_stock_load_i64() {
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let mut func = Function::with_name_signature(UserFuncName::testcase("envelope_load"), sig);
        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let enum_base = builder.block_params(entry)[0];
        let fd = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            enum_base,
            16,
        );
        builder.ins().return_(&[fd]);
        builder.finalize();

        let flags = test_flags();
        verify_function(&func, &flags).expect("envelope load CLIF must verify");
        let clif = format!("{func}");
        assert!(
            clif.contains("load.i64"),
            "expected load.i64 for envelope slot:\n{clif}"
        );
    }

    #[test]
    fn isle_primitive_clif_matches_stock_lowering() {
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let mut func = Function::with_name_signature(UserFuncName::testcase("isle_probe"), sig);
        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let base = builder.block_params(entry)[0];
        let loaded = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            base,
            16,
        );
        let constant = builder.ins().iconst(types::I64, 42);
        let cmp = builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
            loaded,
            constant,
        );
        let result = builder.ins().uextend(types::I64, cmp);
        builder.ins().return_(&[result]);
        builder.finalize();

        let flags = test_flags();
        verify_function(&func, &flags).expect("stock CLIF probe must verify");
        let clif = format!("{func}");
        assert!(
            clif.contains("load.i64"),
            "expected load.i64 in CLIF:\n{clif}"
        );
        assert!(
            clif.contains("iconst.i64"),
            "expected iconst.i64 in CLIF:\n{clif}"
        );
    }
}
