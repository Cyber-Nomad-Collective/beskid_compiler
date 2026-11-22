//! Browser implementations of the canonical string operations used by CLIF.
//!
//! String values keep their native i64 ABI slots as opaque, invocation-scoped
//! handles. The handles point to engine-managed arrays of UTF-8 bytes.

#![allow(non_snake_case)]

mod numbers;

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use beskid_abi::{
    SYM_STR_CONCAT, SYM_STR_EQ, SYM_STR_FROM_BYTES_UTF8, SYM_STR_FROM_I64, SYM_STR_LEN, SYM_STR_NEW, SYM_STR_SLICE,
};
use beskid_codegen::CodegenArtifact;
use beskid_queries::{SemanticTypeId, SourceSignatureType};
use cranelift_codegen::ir::{AbiParam, Signature, types};
use wasm_encoder::{BlockType, DataSection, FieldType, HeapType, InstructionSink, RefType, StorageType, ValType};

use crate::browser_heap::{Functions, NEXT_HANDLE, REMEMBER, loadArrayFromStack, loadHandleFromStack, reference};

const HOST_NEW: &str = "browser_string_new_len";
const HOST_LEN: &str = "browser_string_len";
const HOST_GET: &str = "browser_string_byte_get";
const HOST_SET: &str = "browser_string_byte_set";

pub(crate) const HOST_STRING_HELPERS: [&str; 5] = [HOST_NEW, HOST_LEN, HOST_GET, HOST_SET, "browser_string_assign"];

const STR_CMP: &str = "str_cmp";
pub(crate) const BYTE_AT: &str = "__beskid_browser_string_byte_at";
const HELPERS: [&str; 8] = [
    SYM_STR_NEW,
    SYM_STR_LEN,
    SYM_STR_EQ,
    SYM_STR_CONCAT,
    SYM_STR_SLICE,
    STR_CMP,
    SYM_STR_FROM_I64,
    SYM_STR_FROM_BYTES_UTF8,
];

#[derive(Default)]
pub(crate) struct StringPool {
    bytes: Vec<Vec<u8>>,
    ids: HashMap<String, usize>,
    active: bool,
    host_transfer: bool,
}

impl StringPool {
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn accepts_import(&self, name: &str) -> bool {
        self.is_active() && (HELPERS.contains(&name) || name == BYTE_AT)
    }

    pub(crate) fn enable_host_marshalling(&mut self) {
        self.active = true;
        self.host_transfer = true;
    }

    pub(crate) fn literal_index(&self, symbol: &str) -> Option<usize> {
        self.ids.get(symbol).copied()
    }

    pub(crate) fn literal_matches_length(&self, index: usize, length: i64) -> bool {
        let bytes = &self.bytes[index];
        length == bytes.len() as i64 || (length == 0 && bytes == &[0])
    }

    pub(crate) fn emit_helpers(&self, functions: &mut Functions, string: StringTypes) -> DataSection {
        let mut data = DataSection::new();
        for bytes in &self.bytes {
            data.passive(bytes.iter().copied());
        }
        self.emitNew(functions, string);
        functions.define(Some(SYM_STR_LEN), [ValType::I64], [ValType::I64], |body| {
            body.local_get(0).i64_eqz();
            returnIf(body, 0);
            loadString(body, 0, string);
            body.array_len().i64_extend_i32_u();
        });
        emitEquals(functions, string);
        emitConcat(functions, string);
        emitSlice(functions, string);
        emitCompare(functions, string);
        emitByteAt(functions, string);
        numbers::emit_integer(functions, string);
        if self.host_transfer {
            emitHostMarshalling(functions, string);
        }
        data
    }

    fn emitNew(&self, functions: &mut Functions, string: StringTypes) {
        use ValType::I64;

        functions.define(Some(SYM_STR_NEW), [I64, I64], [I64], |body| {
            body.local_get(1).i64_eqz().if_(BlockType::Empty);
            body.global_get(NEXT_HANDLE).i32_const(0).array_new_default(string.bytes);
            rememberString(body, string);
            body.return_().end();
            for (index, bytes) in self.bytes.iter().enumerate() {
                body.local_get(0).i64_const(index as i64).i64_eq().if_(BlockType::Empty);
                body.local_get(1).i64_const(bytes.len() as i64).i64_ne().if_(BlockType::Empty).unreachable().end();
                body.global_get(NEXT_HANDLE).i32_const(0).i32_const(bytes.len() as i32);
                body.array_new_data(string.bytes, index as u32);
                rememberString(body, string);
                body.return_().end();
            }
            body.unreachable();
        });
    }
}

pub(crate) fn collect(artifact: &mut CodegenArtifact) -> Result<StringPool> {
    let isString = |ty: &SourceSignatureType| *ty == SourceSignatureType::Scalar(SemanticTypeId::STRING);
    let usesStrings = artifact
        .source_signatures
        .values()
        .any(|signature| signature.parameters.iter().chain([&signature.result]).any(isString))
        || artifact.aggregate_source_fields.values().any(|fields| fields.iter().any(isString))
        || artifact.array_static_plans.iter().any(|array| array.element_type == SemanticTypeId::STRING)
        || artifact.functions.iter().any(|lowered| {
            lowered.function.dfg.ext_funcs.values().any(|external| {
                matches!(&external.name, cranelift_codegen::ir::ExternalName::TestCase(name)
                    if HELPERS.iter().any(|helper| helper.as_bytes() == name.raw()))
            })
        });
    let all = std::mem::take(&mut artifact.string_literals).into_iter().collect::<BTreeMap<_, _>>();
    let mut pool = StringPool { active: usesStrings || !all.is_empty(), ..Default::default() };
    for (name, data) in all {
        ensure!(u32::try_from(data.len()).is_ok(), "oversized browser string literal");
        std::str::from_utf8(&data).context("invalid UTF-8 in browser string literal")?;
        pool.ids.insert(name, pool.bytes.len());
        pool.bytes.push(data);
    }
    Ok(pool)
}

pub(crate) fn accepts_signature(name: &str, signature: &Signature) -> bool {
    let count = match name {
        SYM_STR_LEN | SYM_STR_FROM_I64 => 1,
        SYM_STR_SLICE => 3,
        SYM_STR_NEW | SYM_STR_EQ | SYM_STR_CONCAT | STR_CMP | SYM_STR_FROM_BYTES_UTF8 => 2,
        _ => return false,
    };
    signature.params.len() == count
        && signature.params.iter().all(|param| *param == AbiParam::new(types::I64))
        && signature.returns == [AbiParam::new(types::I64)]
}

#[derive(Clone, Copy)]
pub(crate) struct StringTypes {
    bytes: u32,
    pub(crate) object: u32,
}

pub(crate) fn define_types(functions: &mut Functions) -> StringTypes {
    let bytes = functions.types.len();
    functions.types.ty().array(&StorageType::I8, true);
    let object = functions.types.len();
    functions.types.ty().struct_([
        FieldType { element_type: StorageType::Val(ValType::I32), mutable: false },
        FieldType { element_type: StorageType::Val(reference(bytes, false)), mutable: true },
        // Distinguish a string from a u8[] with the same packed storage.
        FieldType { element_type: StorageType::I8, mutable: false },
    ]);
    StringTypes { bytes, object }
}

fn rememberString(body: &mut InstructionSink<'_>, string: StringTypes) {
    body.i32_const(0).struct_new(string.object).call(REMEMBER).i64_extend_i32_u();
}

pub(crate) fn emit_from_utf8_code_units(
    functions: &mut Functions,
    string: StringTypes,
    byteArray: u32,
    byteStorage: u32,
) {
    use ValType::{I32, I64};
    functions.defineWithLocals(
        Some(SYM_STR_FROM_BYTES_UTF8),
        [I64, I64],
        [I64],
        [stringRef(byteStorage), I32, stringRef(string.bytes)],
        |body| {
            body.local_get(0).i64_const(u32::MAX.into()).i64_gt_u().if_(BlockType::Empty).unreachable().end();
            body.local_get(0).i32_wrap_i64();
            loadArrayFromStack(body, byteArray);
            body.local_tee(2).array_len().i64_extend_i32_u().local_get(1).i64_lt_u();
            body.if_(BlockType::Empty).unreachable().end();
            body.local_get(1).i32_wrap_i64().local_tee(3).array_new_default(string.bytes).local_set(4);
            body.local_get(4).i32_const(0).local_get(2).i32_const(0).local_get(3);
            body.array_copy(string.bytes, byteStorage);
            body.global_get(NEXT_HANDLE).local_get(4).ref_as_non_null();
            rememberString(body, string);
        },
    );
}

fn emitSlice(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};
    let array = string.bytes;
    functions.defineWithLocals(
        Some(SYM_STR_SLICE),
        [I64, I64, I64],
        [I64],
        [stringRef(array), I64, I64, I32, stringRef(array)],
        |body| {
            body.local_get(0).i64_eqz();
            returnIf(body, 0);
            loadString(body, 0, string);
            body.local_tee(3).array_len().i64_extend_i32_u().local_set(4);
            body.local_get(1).local_get(4).i64_ge_u().if_(BlockType::Empty);
            body.global_get(NEXT_HANDLE).i32_const(0).array_new_default(array);
            rememberString(body, string);
            body.return_().end();
            body.local_get(1).local_get(2).i64_add().local_set(5);
            body.local_get(5).local_get(1).i64_lt_u();
            returnIf(body, 0);
            body.local_get(4).local_get(5).local_get(4).local_get(5).i64_lt_u().select();
            body.local_get(1).i64_sub().i32_wrap_i64().local_tee(6);
            body.array_new_default(array).local_set(7);
            body.local_get(7).i32_const(0).local_get(3).local_get(1).i32_wrap_i64().local_get(6);
            body.array_copy(array, array);
            body.global_get(NEXT_HANDLE).local_get(7).ref_as_non_null();
            rememberString(body, string);
        },
    );
}

fn emitCompare(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};
    let array = string.bytes;
    functions.defineWithLocals(
        Some(STR_CMP),
        [I64, I64],
        [I64],
        [stringRef(array), stringRef(array), I32, I32, I32, I32, I32],
        |body| {
            body.local_get(0).local_get(1).i64_eq();
            returnIf(body, 0);
            for (handle, reference, length) in [(0, 2, 4), (1, 3, 5)] {
                body.local_get(handle).i64_eqz().if_(BlockType::Empty).else_();
                loadString(body, handle, string);
                body.local_tee(reference).array_len().local_set(length).end();
            }
            body.block(BlockType::Empty).loop_(BlockType::Empty);
            body.local_get(6).local_get(4).i32_ge_u().local_get(6).local_get(5).i32_ge_u().i32_or().br_if(1);
            body.local_get(2).local_get(6).array_get_u(array).local_set(7);
            body.local_get(3).local_get(6).array_get_u(array).local_set(8);
            body.local_get(7).local_get(8).i32_lt_u();
            returnIf(body, -1);
            body.local_get(7).local_get(8).i32_gt_u();
            returnIf(body, 1);
            body.local_get(6).i32_const(1).i32_add().local_set(6).br(0);
            body.end().end();
            body.local_get(4).local_get(5).i32_lt_u();
            returnIf(body, -1);
            body.local_get(4).local_get(5).i32_gt_u().i64_extend_i32_u();
        },
    );
}

fn emitByteAt(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};
    functions.define(Some(BYTE_AT), [I32, I64], [I32], |body| {
        body.local_get(1).i64_const(u32::MAX.into()).i64_gt_u().if_(BlockType::Empty).unreachable().end();
        loadStringBytes(body, 0, string);
        body.local_get(1).i32_wrap_i64().array_get_u(string.bytes);
    });
}

fn emitHostMarshalling(functions: &mut Functions, string: StringTypes) {
    use ValType::I32;
    functions.define(Some(HOST_NEW), [I32], [I32], |body| {
        body.global_get(NEXT_HANDLE).local_get(0).array_new_default(string.bytes);
        body.i32_const(0).struct_new(string.object).call(REMEMBER);
    });
    functions.define(Some("browser_string_assign"), [I32, I32], [], |body| {
        body.local_get(0);
        crate::browser_heap::loadHandleFromStack(body, string.object);
        loadStringBytes(body, 1, string);
        body.struct_set(string.object, 1);
    });
    functions.define(Some(HOST_LEN), [I32], [I32], |body| {
        loadStringBytes(body, 0, string);
        body.array_len();
    });
    functions.define(Some(HOST_GET), [I32, I32], [I32], |body| {
        loadStringBytes(body, 0, string);
        body.local_get(1).array_get_u(string.bytes);
    });
    functions.define(Some(HOST_SET), [I32, I32, I32], [], |body| {
        loadStringBytes(body, 0, string);
        body.local_get(1).local_get(2).array_set(string.bytes);
    });
}

fn loadStringBytes(body: &mut InstructionSink<'_>, local: u32, string: StringTypes) {
    body.local_get(local);
    loadHandleFromStack(body, string.object);
    body.struct_get(string.object, 1);
}

fn returnIf(body: &mut InstructionSink<'_>, value: i64) {
    body.if_(BlockType::Empty).i64_const(value).return_().end();
}

fn loadString(body: &mut InstructionSink<'_>, local: u32, string: StringTypes) {
    body.local_get(local).i64_const(u32::MAX.into()).i64_gt_u().if_(BlockType::Empty).unreachable().end();
    body.local_get(local).i32_wrap_i64();
    loadHandleFromStack(body, string.object);
    body.struct_get(string.object, 1);
}

fn stringRef(array: u32) -> ValType {
    ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(array) })
}

fn emitEquals(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};

    let array = string.bytes;

    functions.defineWithLocals(
        Some(SYM_STR_EQ),
        [I64, I64],
        [I64],
        [stringRef(array), stringRef(array), I32, I32],
        |body| {
            body.local_get(0).local_get(1).i64_eq();
            returnIf(body, 1);
            body.local_get(0).i64_eqz().local_get(1).i64_eqz().i32_or();
            returnIf(body, 0);
            loadString(body, 0, string);
            body.local_tee(2).array_len().local_tee(4);
            loadString(body, 1, string);
            body.local_tee(3).array_len().i32_ne();
            returnIf(body, 0);
            body.block(BlockType::Empty).loop_(BlockType::Empty);
            body.local_get(5).local_get(4).i32_ge_u().br_if(1);
            body.local_get(2).local_get(5).array_get_u(array);
            body.local_get(3).local_get(5).array_get_u(array).i32_ne();
            returnIf(body, 0);
            body.local_get(5).i32_const(1).i32_add().local_set(5).br(0);
            body.end().end().i64_const(1);
        },
    );
}

fn emitConcat(functions: &mut Functions, string: StringTypes) {
    use ValType::{I32, I64};

    let array = string.bytes;

    functions.defineWithLocals(
        Some(SYM_STR_CONCAT),
        [I64, I64],
        [I64],
        [stringRef(array), stringRef(array), I32, I32, I32, stringRef(array)],
        |body| {
            body.local_get(0).i64_eqz().local_get(1).i64_eqz().i32_or();
            returnIf(body, 0);
            loadString(body, 0, string);
            body.local_tee(2).array_len().local_tee(4);
            loadString(body, 1, string);
            body.local_tee(3).array_len().local_tee(5);
            body.i32_add().local_tee(6).local_get(4).i32_lt_u();
            returnIf(body, 0);

            body.local_get(6).array_new_default(array).local_set(7);
            body.local_get(7).i32_const(0).local_get(2).i32_const(0).local_get(4).array_copy(array, array);
            body.local_get(7).local_get(4).local_get(3).i32_const(0).local_get(5).array_copy(array, array);
            body.global_get(NEXT_HANDLE).local_get(7).ref_as_non_null();
            rememberString(body, string);
        },
    );
}
