//! Trusted CLIF primitive emitters for managed-ABI / runtime helpers.
//!
//! Contract: `isle/primitives.isle`. These constructors are plain Rust (no generated
//! ISLE selector) so runtime code can call them without pulling the AST rule engine.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::Offset32;
use cranelift_codegen::ir::{
    BlockArg, InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_frontend::FunctionBuilder;

/// ISLE-aligned trusted CLIF helpers over an active [`FunctionBuilder`].
pub struct ClifPrimitives<'a, 'f> {
    builder: &'a mut FunctionBuilder<'f>,
    scratch_i64_slot: Option<StackSlot>,
}

impl<'a, 'f> ClifPrimitives<'a, 'f> {
    pub fn new(builder: &'a mut FunctionBuilder<'f>) -> Self {
        Self {
            builder,
            scratch_i64_slot: None,
        }
    }

    pub fn builder(&self) -> &FunctionBuilder<'f> {
        self.builder
    }

    pub fn builder_mut(&mut self) -> &mut FunctionBuilder<'f> {
        self.builder
    }

    fn offset32(offset: i64) -> Option<Offset32> {
        i32::try_from(offset).ok().map(Offset32::new)
    }

    fn trusted_load(&mut self, ty: types::Type, base: Value, offset: i64) -> Option<Value> {
        let offset = Self::offset32(offset)?;
        Some(
            self.builder
                .ins()
                .load(ty, MemFlags::trusted(), base, offset),
        )
    }

    fn trusted_store(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        let offset = Self::offset32(offset)?;
        self.builder
            .ins()
            .store(MemFlags::trusted(), val, base, offset);
        Some(val)
    }

    pub fn iconst_i64(&mut self, val: i64) -> Value {
        self.builder.ins().iconst(types::I64, val)
    }

    pub fn load_i64(&mut self, base: Value, offset: i64) -> Option<Value> {
        self.trusted_load(types::I64, base, offset)
    }

    pub fn store_i64(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        self.trusted_store(base, offset, val)
    }

    pub fn load_i8_zext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded = self.trusted_load(types::I8, base, offset)?;
        Some(self.builder.ins().uextend(types::I64, loaded))
    }

    pub fn load_i16_zext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded = self.trusted_load(types::I16, base, offset)?;
        Some(self.builder.ins().uextend(types::I64, loaded))
    }

    pub fn load_i32_zext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded = self.trusted_load(types::I32, base, offset)?;
        Some(self.builder.ins().uextend(types::I64, loaded))
    }

    pub fn load_i16_sext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded = self.trusted_load(types::I16, base, offset)?;
        Some(self.builder.ins().sextend(types::I64, loaded))
    }

    pub fn load_i32_sext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded = self.trusted_load(types::I32, base, offset)?;
        Some(self.builder.ins().sextend(types::I64, loaded))
    }

    pub fn store_i8(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        self.trusted_store(base, offset, val)
    }

    pub fn store_i16(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        self.trusted_store(base, offset, val)
    }

    pub fn store_i32(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        self.trusted_store(base, offset, val)
    }

    /// Explicit-slot `stack_load` for helpers that need addressable i64 scratch (locals use SSA).
    pub fn stack_load_i64(&mut self, offset: i32) -> Value {
        let slot = self.ensure_scratch_i64_slot();
        self.builder.ins().stack_load(types::I64, slot, offset)
    }

    pub fn stack_store_i64(&mut self, val: Value, offset: i32) -> Value {
        let slot = self.ensure_scratch_i64_slot();
        self.builder.ins().stack_store(val, slot, offset);
        val
    }

    fn ensure_scratch_i64_slot(&mut self) -> StackSlot {
        if let Some(slot) = self.scratch_i64_slot {
            return slot;
        }
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        self.scratch_i64_slot = Some(slot);
        slot
    }

    pub fn ptr_add(&mut self, base: Value, imm: i64) -> Value {
        self.builder.ins().iadd_imm(base, imm)
    }

    pub fn icmp_eq(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::Equal, left, right)
    }

    pub fn icmp_ne(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::NotEqual, left, right)
    }

    pub fn icmp_slt(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::SignedLessThan, left, right)
    }

    pub fn icmp_ult(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().icmp(IntCC::UnsignedLessThan, left, right)
    }

    pub fn icmp_ule(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, left, right)
    }

    pub fn icmp_ugt(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, left, right)
    }

    pub fn icmp_uge(&mut self, left: Value, right: Value) -> Value {
        self.builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, left, right)
    }

    pub fn fadd_f64(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().fadd(left, right)
    }

    pub fn fsub_f64(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().fsub(left, right)
    }

    pub fn fmul_f64(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().fmul(left, right)
    }

    pub fn fdiv_f64(&mut self, left: Value, right: Value) -> Value {
        self.builder.ins().fdiv(left, right)
    }

    pub fn fneg_f64(&mut self, value: Value) -> Value {
        self.builder.ins().fneg(value)
    }

    pub fn fcvt_from_sint_f64(&mut self, value: Value) -> Value {
        self.builder.ins().fcvt_from_sint(types::F64, value)
    }

    pub fn fcvt_to_sint_i64(&mut self, value: Value) -> Value {
        self.builder.ins().fcvt_to_sint(types::I64, value)
    }

    pub fn icmp_byte_ne(
        &mut self,
        left: Value,
        right: Value,
        left_off: i64,
        right_off: i64,
    ) -> Option<Value> {
        let lb = self.load_i8_zext(left, left_off)?;
        let rb = self.load_i8_zext(right, right_off)?;
        Some(self.icmp_ne(lb, rb))
    }

    pub fn bounded_memcmp(&mut self, left: Value, right: Value, len: Value) -> Option<Value> {
        let builder = &mut *self.builder;
        let header = builder.current_block()?;
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I64);

        let zero = builder.ins().iconst(types::I64, 0);
        let neg_one = builder.ins().iconst(types::I64, -1);
        let one = builder.ins().iconst(types::I64, 1);

        let len_is_zero = builder.ins().icmp(IntCC::Equal, len, zero);
        let loop_block = builder.create_block();
        builder.append_block_param(loop_block, types::I64);
        let len_zero_done = builder.create_block();
        builder.ins().brif(
            len_is_zero,
            len_zero_done,
            &[],
            loop_block,
            &[BlockArg::Value(zero)],
        );

        builder.switch_to_block(len_zero_done);
        builder.ins().jump(merge, &[BlockArg::Value(zero)]);

        builder.switch_to_block(loop_block);
        let idx = builder.block_params(loop_block)[0];
        let idx_lt_len = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, idx, len);
        let body = builder.create_block();
        let done = builder.create_block();
        builder.ins().brif(idx_lt_len, body, &[], done, &[]);

        builder.switch_to_block(body);
        let lptr = builder.ins().iadd(left, idx);
        let rptr = builder.ins().iadd(right, idx);
        let lb = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), lptr, Offset32::new(0));
        let rb = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), rptr, Offset32::new(0));
        let lb64 = builder.ins().uextend(types::I64, lb);
        let rb64 = builder.ins().uextend(types::I64, rb);
        let bytes_eq = builder.ins().icmp(IntCC::Equal, lb64, rb64);
        let mismatch = builder.create_block();
        let next_idx = builder.ins().iadd_imm(idx, 1);
        builder.ins().brif(
            bytes_eq,
            loop_block,
            &[BlockArg::Value(next_idx)],
            mismatch,
            &[],
        );

        builder.switch_to_block(mismatch);
        let left_lt = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, lb64, rb64);
        let mismatch_result = builder.ins().select(left_lt, neg_one, one);
        builder
            .ins()
            .jump(merge, &[BlockArg::Value(mismatch_result)]);

        builder.switch_to_block(done);
        builder.ins().jump(merge, &[BlockArg::Value(zero)]);

        builder.seal_block(loop_block);
        builder.seal_block(body);
        builder.seal_block(mismatch);
        builder.seal_block(done);
        builder.seal_block(len_zero_done);
        builder.seal_block(merge);

        builder.switch_to_block(merge);
        let result = builder.block_params(merge)[0];
        builder.switch_to_block(header);
        Some(result)
    }
}
