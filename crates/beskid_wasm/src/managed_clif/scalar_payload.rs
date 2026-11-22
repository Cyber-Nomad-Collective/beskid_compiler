//! Inline CLIF addresses scalar array payloads in bytes. Keep these operations
//! on the typed GC array, including little-endian loads spanning elements.
use super::*;
use cranelift_codegen::ir::{MemFlagsData, Type};

#[derive(Clone, Copy)]
pub(super) struct Payload {
    pub(super) handle: Value,
    pub(super) kind: Scalar,
    offset: Offset,
}

#[derive(Clone, Copy)]
enum Offset {
    Base,
    Element(Index),
    Bytes(Value),
}

impl Pass<'_> {
    pub(super) fn big_endian_payload(&self, inst: Inst) -> bool {
        self.function.dfg.insts[inst].memflags_data(&self.function.dfg).and_then(|flags| flags.explicit_endianness())
            == Some(cranelift_codegen::ir::Endianness::Big)
    }

    fn payload_endian(&mut self, inst: Inst, value: Value) -> Value {
        let ty = self.function.dfg.value_type(value);
        if ty.bytes() == 1 || !self.big_endian_payload(inst) {
            return value;
        }
        let mut cursor = self.rewindBefore(inst);
        let bits = if ty.is_float() { cursor.ins().bitcast(ty.as_int(), MemFlagsData::new(), value) } else { value };
        let swapped = cursor.ins().bswap(bits);
        if ty.is_float() { cursor.ins().bitcast(ty, MemFlagsData::new(), swapped) } else { swapped }
    }

    pub(super) fn scalar_payload(&self, address: Value) -> Option<Payload> {
        let (handle, element, offset) = match self.source(address)? {
            Native::ArrayData(h, e) => (h, e, Offset::Base),
            Native::Element(h, index, e) => (h, e, Offset::Element(index)),
            Native::ArrayByteOffset(h, bytes, e) => (h, e, Offset::Bytes(bytes)),
            _ => return None,
        };
        let ArrayElement::Scalar(kind) = element else { return None };
        Some(Payload { handle, kind, offset })
    }

    pub(super) fn payload_offset(&mut self, inst: Inst, payload: Payload, displacement: i64) -> Value {
        let stride = i64::from(scalar_type(payload.kind).bytes());
        let base = match payload.offset {
            Offset::Base => return self.rewindBefore(inst).ins().iconst(I64, displacement),
            Offset::Bytes(bytes) => bytes,
            Offset::Element(index) => {
                let index = self.materializeIndex(inst, index);
                let mut cursor = self.rewindBefore(inst);
                let word = cursor.ins().uextend(I64, index);
                if stride == 1 { word } else { cursor.ins().imul_imm(word, stride) }
            }
        };
        if displacement == 0 { base } else { self.rewindBefore(inst).ins().iadd_imm(base, displacement) }
    }

    fn payload_element(&mut self, inst: Inst, position: Value, width: u32) -> (Value, Value) {
        let mut cursor = self.rewindBefore(inst);
        let index = cursor.ins().ushr_imm(position, i64::from(width.trailing_zeros()));
        let lane = cursor.ins().band_imm(position, i64::from(width - 1));
        let shift = cursor.ins().ishl_imm(lane, 3);
        (self.materializeIndex(inst, Index::Word(index)), shift)
    }

    fn load_word(&mut self, inst: Inst, p: Payload, index: Value) -> Value {
        let ty = scalar_type(p.kind);
        let get = ArrayOp::Get.symbol(ArrayElement::Scalar(p.kind));
        let call = self.callBefore(inst, &get, &[p.handle, index], Some(ty));
        self.function.dfg.first_result(call)
    }

    fn as_word(&mut self, inst: Inst, value: Value) -> Value {
        let ty = self.function.dfg.value_type(value);
        let mut cursor = self.rewindBefore(inst);
        let bits = if ty.is_float() { cursor.ins().bitcast(ty.as_int(), MemFlagsData::new(), value) } else { value };
        if ty.bits() == 64 { bits } else { cursor.ins().uextend(I64, bits) }
    }

    fn from_word(&mut self, inst: Inst, value: Value, ty: Type) -> Value {
        let mut cursor = self.rewindBefore(inst);
        let bits = if ty.bits() == 64 { value } else { cursor.ins().ireduce(ty.as_int(), value) };
        if ty.is_float() { cursor.ins().bitcast(ty, MemFlagsData::new(), bits) } else { bits }
    }

    fn aligned_payload_index(&mut self, inst: Inst, p: Payload, disp: i64, ty: Type) -> Option<Value> {
        let width = i64::from(ty.bytes());
        if ty != scalar_type(p.kind) || disp % width != 0 {
            return None;
        }
        let base = match p.offset {
            Offset::Base => {
                let index = u32::try_from(disp / width).ok()?;
                return Some(self.rewindBefore(inst).ins().iconst(I32, i64::from(index)));
            }
            Offset::Element(index) => {
                let index = self.materializeIndex(inst, index);
                self.rewindBefore(inst).ins().uextend(I64, index)
            }
            Offset::Bytes(bytes) if width == 1 => bytes,
            _ => return None,
        };
        let shifted = self.rewindBefore(inst).ins().iadd_imm(base, disp / width);
        Some(self.materializeIndex(inst, Index::Word(shifted)))
    }

    pub(super) fn load_extended_payload(
        &mut self, inst: Inst, payload: Payload, offset: i64, opcode: Opcode,
    ) -> Result<Edit, &'static str> {
        let (width, signed) = match opcode {
            Opcode::Uload8 => (I8, false),
            Opcode::Sload8 => (I8, true),
            Opcode::Uload16 => (cranelift_codegen::ir::types::I16, false),
            Opcode::Sload16 => (cranelift_codegen::ir::types::I16, true),
            Opcode::Uload32 => (I32, false),
            Opcode::Sload32 => (I32, true),
            _ => return Err("not an extending scalar load"),
        };
        let value = self.read_payload_scalar(inst, payload, offset, width)?;
        let result_type = self.function.dfg.value_type(self.function.dfg.first_result(inst));
        if signed {
            self.function.replace(inst).sextend(result_type, value);
        } else {
            self.function.replace(inst).uextend(result_type, value);
        }
        Ok(Edit::Replace)
    }

    pub(super) fn load_payload(&mut self, inst: Inst, p: Payload, disp: i64, ty: Type) -> Result<Edit, &'static str> {
        let value = if ty.is_vector() {
            let lane_ty = ty.lane_type();
            let first = self.read_payload_scalar(inst, p, disp, lane_ty)?;
            let mut result = self.rewindBefore(inst).ins().splat(ty, first);
            for lane in 1..ty.lane_count() {
                let offset = disp + i64::from(lane * lane_ty.bytes());
                let value = self.read_payload_scalar(inst, p, offset, lane_ty)?;
                result = self.rewindBefore(inst).ins().insertlane(result, value, lane as u8);
            }
            result
        } else {
            self.read_payload_scalar(inst, p, disp, ty)?
        };
        let dfg = &mut self.function.dfg;
        let result = dfg.first_result(inst);
        dfg.clear_results(inst);
        dfg.change_to_alias(result, value);
        Ok(Edit::Erase)
    }

    fn read_payload_scalar(&mut self, inst: Inst, p: Payload, disp: i64, ty: Type) -> Result<Value, &'static str> {
        if !matches!(ty.bytes(), 1 | 2 | 4 | 8) || !(ty.is_int() || ty.is_float()) {
            return Err("unsupported inline CLIF scalar payload load width");
        }
        let value = if let Some(index) = self.aligned_payload_index(inst, p, disp, ty) {
            self.load_word(inst, p, index)
        } else {
            let position = self.payload_offset(inst, p, disp);
            let mut result = self.rewindBefore(inst).ins().iconst(I64, 0);
            for byte in 0..ty.bytes() {
                let at = self.rewindBefore(inst).ins().iadd_imm(position, i64::from(byte));
                let (index, shift) = self.payload_element(inst, at, scalar_type(p.kind).bytes());
                let source = self.load_word(inst, p, index);
                let word = self.as_word(inst, source);
                let mut cursor = self.rewindBefore(inst);
                let shifted = cursor.ins().ushr(word, shift);
                let lane = cursor.ins().band_imm(shifted, 255);
                let contribution = cursor.ins().ishl_imm(lane, i64::from(byte * 8));
                result = cursor.ins().bor(result, contribution);
            }
            self.from_word(inst, result, ty)
        };
        Ok(self.payload_endian(inst, value))
    }

    pub(super) fn store_payload(
        &mut self,
        inst: Inst,
        p: Payload,
        disp: i64,
        value: Value,
    ) -> Result<Edit, &'static str> {
        let ty = self.function.dfg.value_type(value);
        if ty.is_vector() {
            let last = self.payload_offset(inst, p, disp + i64::from(ty.bytes() - 1));
            let (index, _) = self.payload_element(inst, last, scalar_type(p.kind).bytes());
            self.load_word(inst, p, index);
            let lane_ty = ty.lane_type();
            for lane in 0..ty.lane_count() {
                let part = self.rewindBefore(inst).ins().extractlane(value, lane as u8);
                self.store_payload(inst, p, disp + i64::from(lane * lane_ty.bytes()), part)?;
            }
            return Ok(Edit::Erase);
        }
        if self.source(value).is_some() || !matches!(ty.bytes(), 1 | 2 | 4 | 8) || !(ty.is_int() || ty.is_float()) {
            return Err("unsupported inline CLIF scalar payload store");
        }
        let value = self.payload_endian(inst, value);
        let set = ArrayOp::Set.symbol(ArrayElement::Scalar(p.kind));
        if let Some(index) = self.aligned_payload_index(inst, p, disp, ty) {
            self.callBefore(inst, &set, &[p.handle, index, value], None);
        } else {
            let position = self.payload_offset(inst, p, disp);
            let word = self.as_word(inst, value);
            let element_ty = scalar_type(p.kind);
            // Check the last element first so a failed wide store never writes a partial word.
            let last = self.rewindBefore(inst).ins().iadd_imm(position, i64::from(ty.bytes() - 1));
            let (end, _) = self.payload_element(inst, last, element_ty.bytes());
            self.load_word(inst, p, end);
            for byte in 0..ty.bytes() {
                let at = self.rewindBefore(inst).ins().iadd_imm(position, i64::from(byte));
                let (index, shift) = self.payload_element(inst, at, element_ty.bytes());
                if element_ty == I8 {
                    let mut cursor = self.rewindBefore(inst);
                    let shifted = cursor.ins().ushr_imm(word, i64::from(byte * 8));
                    let new = cursor.ins().ireduce(I8, shifted);
                    self.callBefore(inst, &set, &[p.handle, index, new], None);
                    continue;
                }
                let old = self.load_word(inst, p, index);
                let old = self.as_word(inst, old);
                let mut cursor = self.rewindBefore(inst);
                let lane = cursor.ins().ushr_imm(word, i64::from(byte * 8));
                let lane = cursor.ins().band_imm(lane, 255);
                let mask = cursor.ins().iconst(I64, 255);
                let mask = cursor.ins().ishl(mask, shift);
                let preserved = cursor.ins().band_not(old, mask);
                let bits = cursor.ins().ishl(lane, shift);
                let merged = cursor.ins().bor(preserved, bits);
                let new = self.from_word(inst, merged, element_ty);
                self.callBefore(inst, &set, &[p.handle, index, new], None);
            }
        }
        Ok(Edit::Erase)
    }
}
