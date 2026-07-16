//! Bridges generated ISLE constructors to stock Cranelift CLIF via [`FunctionBuilder`].

use cranelift_codegen::ir::immediates::Offset32;
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

/// ISLE `Context` implementation: emits stock CLIF through an active [`FunctionBuilder`].
pub struct IsleContext<'a> {
    builder: &'a mut FunctionBuilder<'a>,
}

impl<'a> IsleContext<'a> {
    pub fn new(builder: &'a mut FunctionBuilder<'a>) -> Self {
        Self { builder }
    }

    pub fn builder(&self) -> &FunctionBuilder<'a> {
        self.builder
    }

    pub fn builder_mut(&mut self) -> &mut FunctionBuilder<'a> {
        self.builder
    }

    fn offset32(offset: i64) -> Offset32 {
        Offset32::new(i32::try_from(offset).expect("ISLE offset must fit i32"))
    }
}

impl crate::isle_generated::Context for IsleContext<'_> {
    fn iconst_i64(&mut self, val: i64) -> Option<Value> {
        Some(self.builder.ins().iconst(types::I64, val))
    }

    fn load_i64(&mut self, base: Value, offset: i64) -> Option<Value> {
        Some(self.builder.ins().load(
            types::I64,
            MemFlags::trusted(),
            base,
            Self::offset32(offset),
        ))
    }

    fn store_i64(&mut self, base: Value, offset: i64, val: Value) -> Option<Value> {
        self.builder
            .ins()
            .store(MemFlags::trusted(), base, val, Self::offset32(offset));
        Some(val)
    }

    fn load_i8_zext(&mut self, base: Value, offset: i64) -> Option<Value> {
        let loaded =
            self.builder
                .ins()
                .load(types::I8, MemFlags::trusted(), base, Self::offset32(offset));
        Some(self.builder.ins().uextend(types::I64, loaded))
    }

    fn ptr_add(&mut self, base: Value, imm: i64) -> Option<Value> {
        Some(self.builder.ins().iadd_imm(base, imm))
    }

    fn icmp_eq(&mut self, left: Value, right: Value) -> Option<Value> {
        Some(
            self.builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, left, right),
        )
    }

    fn icmp_ne(&mut self, left: Value, right: Value) -> Option<Value> {
        Some(self.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            left,
            right,
        ))
    }

    fn icmp_slt(&mut self, left: Value, right: Value) -> Option<Value> {
        Some(self.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
            left,
            right,
        ))
    }

    fn icmp_byte_ne(
        &mut self,
        left: Value,
        right: Value,
        left_off: i64,
        right_off: i64,
    ) -> Option<Value> {
        let lb = self.load_i8_zext(left, left_off)?;
        let rb = self.load_i8_zext(right, right_off)?;
        self.icmp_ne(lb, rb)
    }

    fn bounded_memcmp(&mut self, left: Value, right: Value, len: Value) -> Option<Value> {
        let builder = &mut self.builder;
        let header = builder.current_block()?;
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I64);

        let zero = builder.ins().iconst(types::I64, 0);
        let neg_one = builder.ins().iconst(types::I64, -1);
        let one = builder.ins().iconst(types::I64, 1);

        let len_is_zero =
            builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, len, zero);
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
        let idx_lt_len = builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan,
            idx,
            len,
        );
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
        let bytes_eq =
            builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, lb64, rb64);
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
        let left_lt = builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan,
            lb64,
            rb64,
        );
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
