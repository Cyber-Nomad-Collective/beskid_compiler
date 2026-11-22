//! Wasm GC types and typed Wasm helpers for the managed CLIF adapter.

#![allow(non_snake_case)]

use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod events;
pub(crate) mod observers;

use crate::strings::{self, StringPool, StringTypes};
use beskid_abi::SYM_GC_COLLECT_IF_NEEDED;
use wasm_encoder::{
    ArrayType, BlockType, CodeSection, CompositeInnerType, CompositeType, DataCountSection, ExportKind, ExportSection,
    FieldType, Function, FunctionSection, HeapType, InstructionSink, Module, RefType, StorageType, StructType, SubType,
    TypeSection, ValType,
};

pub(crate) const NEXT_HANDLE: u32 = 0;
pub(crate) const REMEMBER: u32 = 0;
const HANDLE_ID_FIELD: u32 = 0;
const ARRAY_DATA_FIELD: u32 = 1;
const ARRAY_LENGTH_FIELD: u32 = 2;
const ARRAY_SHARED_FIELD: u32 = 3;
pub(crate) const RESET_HANDLES: &str = "reset_handles";
pub(crate) const RELEASE_HANDLE: &str = "__beskid_browser_release_handle";
pub(crate) const IDENTITY_STATUS: &str = "__beskid_browser_identity_status";
pub(crate) const REFERENCE_EQUAL: &str = "__beskid_browser_reference_equal";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scalar {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl Scalar {
    fn wasm(self) -> ValType {
        match self {
            Self::I8 | Self::I16 | Self::I32 => ValType::I32,
            Self::I64 => ValType::I64,
            Self::F32 => ValType::F32,
            Self::F64 => ValType::F64,
        }
    }

    fn storage(self) -> StorageType {
        match self {
            Self::I8 => StorageType::I8,
            Self::I16 => StorageType::I16,
            _ => StorageType::Val(self.wasm()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArrayElement {
    Scalar(Scalar),
    String,
    Record(usize),
    Nested { leaf: ArrayLeaf, depth: std::num::NonZeroUsize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArrayLeaf {
    Scalar(Scalar),
    String,
    Record(usize),
}

impl ArrayElement {
    pub(crate) fn nested(self) -> Option<Self> {
        let (leaf, depth) = match self {
            Self::Scalar(kind) => (ArrayLeaf::Scalar(kind), 1),
            Self::String => (ArrayLeaf::String, 1),
            Self::Record(record) => (ArrayLeaf::Record(record), 1),
            Self::Nested { leaf, depth } => (leaf, depth.get().checked_add(1)?),
        };
        Some(Self::Nested { leaf, depth: depth.try_into().ok()? })
    }

    pub(crate) fn inner(self) -> Option<Self> {
        let Self::Nested { leaf, depth } = self else { return None };
        Some(match std::num::NonZeroUsize::new(depth.get() - 1) {
            Some(depth) => Self::Nested { leaf, depth },
            None => match leaf {
                ArrayLeaf::Scalar(kind) => Self::Scalar(kind),
                ArrayLeaf::String => Self::String,
                ArrayLeaf::Record(record) => Self::Record(record),
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Scalar(Scalar),
    Array(ArrayElement),
    String,
    Record(usize),
    /// Nullable reference whose concrete record type is proven at dispatch.
    AnyRecord,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeapLayout {
    pub arrays: BTreeSet<ArrayElement>,
    pub records: Vec<Vec<Field>>,
    pub observers: Option<observers::Observers>,
    pub(crate) events: Vec<events::Event>,
}

impl HeapLayout {
    pub(crate) fn arrayElements(&self) -> BTreeSet<ArrayElement> {
        let elements = self.arrays
            .iter()
            .copied()
            .chain(self.records.iter().flatten().filter_map(|field| match field {
                Field::Scalar(_) | Field::String | Field::Record(_) | Field::AnyRecord => None,
                Field::Array(element) => Some(*element),
            }))
            .collect::<Vec<_>>();
        elements.into_iter().flat_map(|element| std::iter::successors(Some(element), |e| e.inner())).collect()
    }

    pub fn import_names(&self) -> Vec<String> {
        let mut names = vec![RESET_HANDLES.to_owned(), SYM_GC_COLLECT_IF_NEEDED.to_owned()];
        names.extend(crate::arena::EXPORTS.map(str::to_owned));
        names.push(REFERENCE_EQUAL.to_owned());
        names.push(IDENTITY_STATUS.to_owned());
        for &event in &self.events {
            names.extend(events::Operation::ALL.map(|operation| operation.name(event)));
        }
        if self.observers.is_some() {
            names.extend([observers::WASM_SUBSCRIBE.to_owned(), observers::WASM_TAIL.to_owned()]);
        }
        if !self.records.is_empty() || !self.arrays.is_empty() {
            names.push(RELEASE_HANDLE.to_owned());
        }
        for element in self.arrayElements() {
            names.extend(ArrayOp::ALL.map(|op| op.symbol(element)));
        }
        for (record, fields) in self.records.iter().enumerate() {
            names.push(RecordOp::Alloc.symbol(record));
            for index in 0..fields.len() {
                names.push(RecordOp::Get(index).symbol(record));
                names.push(RecordOp::Set(index).symbol(record));
            }
        }
        names
    }
}

#[derive(Clone, Copy)]
pub enum ArrayOp {
    Alloc,
    Get,
    Set,
    Len,
    Capacity,
    Grow,
    TryGrow,
    SetLen,
    Shared,
    MarkShared,
}

impl ArrayOp {
    const ALL: [Self; 10] = [Self::Alloc, Self::Get, Self::Set, Self::Len, Self::Capacity, Self::Grow, Self::TryGrow, Self::SetLen, Self::Shared, Self::MarkShared];

    pub fn symbol(self, element: ArrayElement) -> String {
        let op = match self {
            Self::Alloc => "alloc",
            Self::Get => "get",
            Self::Set => "set",
            Self::Len => "len",
            Self::Capacity => "capacity",
            Self::Grow => "grow",
            Self::TryGrow => "try_grow",
            Self::SetLen => "set_len",
            Self::Shared => "shared",
            Self::MarkShared => "mark_shared",
        };
        match element {
            ArrayElement::Scalar(scalar) => format!("array_{}_{op}", scalar.name()),
            ArrayElement::String => format!("array_string_{op}"),
            ArrayElement::Record(record) => format!("array_record_{record}_{op}"),
            ArrayElement::Nested { leaf, depth } => {
                let leaf = match leaf {
                    ArrayLeaf::Scalar(scalar) => scalar.name().to_owned(),
                    ArrayLeaf::String => "string".to_owned(),
                    ArrayLeaf::Record(record) => format!("record_{record}"),
                };
                format!("array_nested_{depth}_{leaf}_{op}")
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum RecordOp {
    Alloc,
    Get(usize),
    Set(usize),
}

impl RecordOp {
    pub fn symbol(self, record: usize) -> String {
        match self {
            Self::Alloc => format!("record_{record}_alloc"),
            Self::Get(field) => format!("record_{record}_get_{field}"),
            Self::Set(field) => format!("record_{record}_set_{field}"),
        }
    }
}

#[derive(Clone, Copy)]
struct ArrayTypes {
    data: u32,
    object: u32,
}

pub(crate) fn reference(typeIndex: u32, nullable: bool) -> ValType {
    ValType::Ref(RefType { nullable, heap_type: HeapType::Concrete(typeIndex) })
}

/// Handles are positive, invocation-scoped table indices. `reset_handles`
/// drops all roots. An array's wrapper stores its original handle, so reading
/// an array field returns the same handle without rooting the object again.
pub fn encode(layout: &HeapLayout) -> Vec<u8> {
    encode_with_strings(layout, &StringPool::default())
}

pub(crate) fn encode_with_strings(layout: &HeapLayout, strings: &StringPool) -> Vec<u8> {
    use ValType::{I32, I64};

    let mut functions = Functions::default();
    let globals = crate::arena::define(&mut functions);
    let mut arrayTypes = BTreeMap::new();
    let arrayElements = layout.arrayElements();
    let stringTypes = (strings.is_active()
        || arrayElements.contains(&ArrayElement::String)
        || layout.records.iter().flatten().any(|field| *field == Field::String))
    .then(|| strings::define_types(&mut functions));
    for &element in &arrayElements {
        let data = functions.types.len();
        let elementType = match element {
            ArrayElement::Scalar(scalar) => scalar.storage(),
            ArrayElement::String => {
                StorageType::Val(reference(stringTypes.expect("string element type defined").object, true))
            }
            ArrayElement::Record(_) | ArrayElement::Nested { .. } => continue,
        };
        functions.types.ty().array(&elementType, true);
        let object = functions.types.len();
        functions.types.ty().struct_([
            FieldType { element_type: StorageType::Val(I32), mutable: false },
            FieldType { element_type: StorageType::Val(reference(data, false)), mutable: false },
            FieldType { element_type: StorageType::Val(ValType::I32), mutable: true },
            FieldType { element_type: StorageType::I8, mutable: true },
        ]);
        arrayTypes.insert(element, ArrayTypes { data, object });
    }

    let recordArrays = arrayElements.into_iter().filter(|element| matches!(element, ArrayElement::Record(_) | ArrayElement::Nested { .. }));
    let recordTypes = defineRecordTypes(&mut functions, &layout.records, &mut arrayTypes, recordArrays, stringTypes);

    for (&element, &array) in &arrayTypes {
        emitArrayHelpers(&mut functions, element, array, &arrayTypes, stringTypes, &recordTypes);
    }
    for (record, (&recordType, fields)) in recordTypes.iter().zip(&layout.records).enumerate() {
        emitRecordHelpers(&mut functions, record, recordType, fields, &arrayTypes, stringTypes, &recordTypes);
    }
    for &event in &layout.events {
        events::emit(&mut functions, event, &recordTypes, &arrayTypes);
    }
    if let Some(observers) = &layout.observers {
        observers::emit(&mut functions, observers, &recordTypes);
    }
    if !layout.records.is_empty() || !layout.arrays.is_empty() {
        crate::arena::release(&mut functions);
    }
    let stringData = strings
        .is_active()
        .then(|| strings.emit_helpers(&mut functions, stringTypes.expect("active string type defined")));
    if strings.is_active() {
        if let Some(byteArray) = arrayTypes.get(&ArrayElement::Scalar(Scalar::I8)) {
            strings::emit_from_utf8_code_units(
                &mut functions,
                stringTypes.expect("active string type defined"),
                byteArray.object,
                byteArray.data,
            );
        }
    }
    // Preserve the CLIF poll signature. The Wasm engine manages the objects.
    functions.define(Some(REFERENCE_EQUAL), [I32, I32], [I32], |body| {
        body.local_get(0).call(crate::arena::LOOKUP).local_get(1).call(crate::arena::LOOKUP).ref_eq();
    });
    functions.define(Some(IDENTITY_STATUS), [I32, I32], [I64], |body| {
        body.local_get(0).i32_eqz().local_get(1).i32_eqz().i32_or()
            .if_(BlockType::Result(I64)).i64_const(2).else_();
        body.local_get(0).call(crate::arena::LOOKUP).local_get(1).call(crate::arena::LOOKUP)
            .ref_eq().i64_extend_i32_u().end();
    });
    functions.define(Some(SYM_GC_COLLECT_IF_NEEDED), [], [I64], |body| {
        body.i64_const(0);
    });

    let mut module = Module::new();
    module.section(&functions.types).section(&functions.declarations).section(&globals).section(&functions.exports);
    if let Some(data) = &stringData {
        module.section(&DataCountSection { count: data.len() });
    }
    module.section(&functions.code);
    if let Some(data) = stringData {
        module.section(&data);
    }
    module.finish()
}

fn emitArrayHelpers(
    functions: &mut Functions,
    element: ArrayElement,
    array: ArrayTypes,
    arrays: &BTreeMap<ArrayElement, ArrayTypes>,
    string: Option<StringTypes>,
    records: &[u32],
) {
    use ValType::I32;
    functions.define(Some(&ArrayOp::Alloc.symbol(element)), [I32], [I32], |body| {
        body.global_get(NEXT_HANDLE).local_get(0).array_new_default(array.data).local_get(0).i32_const(0).struct_new(array.object).call(REMEMBER);
    });
    match element {
        ArrayElement::Scalar(scalar) => {
            let abiType = scalar.wasm();
            functions.define(Some(&ArrayOp::Get.symbol(element)), [I32, I32], [abiType], |body| {
                loadArray(body, array.object);
                body.local_get(1);
                if matches!(scalar, Scalar::I8 | Scalar::I16) {
                    body.array_get_u(array.data);
                } else {
                    body.array_get(array.data);
                }
            });
            functions.define(Some(&ArrayOp::Set.symbol(element)), [I32, I32, abiType], [], |body| {
                loadArray(body, array.object);
                body.local_get(1).local_get(2).array_set(array.data);
            });
        }
        ArrayElement::String | ArrayElement::Record(_) | ArrayElement::Nested { .. } => {
            let object = match element {
                ArrayElement::String => string.expect("string element type defined").object,
                ArrayElement::Record(record) => records[record],
                ArrayElement::Nested { .. } => arrays[&element.inner().expect("nested array element")].object,
                ArrayElement::Scalar(_) => unreachable!(),
            };
            functions.defineWithLocals(
                Some(&ArrayOp::Get.symbol(element)),
                [I32, I32],
                [I32],
                [reference(object, true)],
                |body| {
                    loadArray(body, array.object);
                    body.local_get(1).array_get(array.data);
                    nullableReferenceToHandle(body, 2, object);
                },
            );
            functions.define(Some(&ArrayOp::Set.symbol(element)), [I32, I32, I32], [], |body| {
                loadArray(body, array.object);
                body.local_get(1);
                nullableHandleToReference(body, 2, object);
                body.array_set(array.data);
            });
        }
    }
    functions.define(Some(&ArrayOp::Len.symbol(element)), [I32], [I32], |body| {
        body.local_get(0).i32_eqz().if_(BlockType::Empty).i32_const(0).return_().end();
        loadHandle(body, 0, array.object);
        body.struct_get(array.object, ARRAY_LENGTH_FIELD);
    });
    functions.define(Some(&ArrayOp::Shared.symbol(element)), [I32], [ValType::I64], |body| {
        body.local_get(0).i32_eqz().if_(BlockType::Empty).i64_const(0).return_().end();
        loadHandle(body, 0, array.object);
        body.struct_get_u(array.object, ARRAY_SHARED_FIELD).i64_extend_i32_u();
    });
    functions.define(Some(&ArrayOp::MarkShared.symbol(element)), [I32], [], |body| {
        body.local_get(0).i32_eqz().if_(BlockType::Empty).return_().end();
        loadHandle(body, 0, array.object);
        body.i32_const(1).struct_set(array.object, ARRAY_SHARED_FIELD);
    });
    functions.define(Some(&ArrayOp::SetLen.symbol(element)), [I32, I32], [], |body| {
        body.local_get(1);
        loadArray(body, array.object);
        body.array_len().i32_gt_u().if_(BlockType::Empty).unreachable().end();
        loadHandle(body, 0, array.object);
        body.local_get(1).struct_set(array.object, ARRAY_LENGTH_FIELD);
    });
    functions.define(Some(&ArrayOp::Capacity.symbol(element)), [I32], [I32], |body| {
        loadArray(body, array.object);
        body.array_len();
    });
    let grow = functions.declarations.len();
    functions.defineWithLocals(Some(&ArrayOp::Grow.symbol(element)), [I32, I32], [I32],
        [reference(array.data, true), I32, reference(array.data, true), I32], |body| {
        loadArray(body, array.object);
        body.local_tee(2).array_len().local_set(3);
        loadHandle(body, 0, array.object);
        body.struct_get(array.object, ARRAY_LENGTH_FIELD).local_set(5);
        body.local_get(1).local_get(5).i32_lt_u().if_(BlockType::Empty).unreachable().end();
        body.local_get(1).local_get(3).i32_le_u().if_(BlockType::Empty).local_get(0).return_().end();
        // Double the logical length, as GrowArray does. A forced copy with spare capacity must not double that capacity.
        body.local_get(5).i32_const(1).i32_shl().local_tee(3).local_get(1).i32_lt_u()
            .if_(BlockType::Empty).local_get(1).local_set(3).end();
        body.local_get(3).i32_const(4).i32_lt_u().if_(BlockType::Empty).i32_const(4).local_set(3).end();
        body.local_get(3).array_new_default(array.data).local_tee(4).i32_const(0)
            .local_get(2).i32_const(0).local_get(5).array_copy(array.data, array.data);
        body.global_get(NEXT_HANDLE).local_get(4).ref_as_non_null().local_get(5).i32_const(0).struct_new(array.object).call(REMEMBER);
    });
    functions.define(Some(&ArrayOp::TryGrow.symbol(element)), [I32, ValType::I64], [I32], |body| {
        body.local_get(0).i32_eqz().if_(BlockType::Empty).i32_const(0).return_().end();
        body.local_get(1).i64_const(i64::from(u32::MAX)).i64_gt_u()
            .if_(BlockType::Empty).i32_const(0).return_().end();
        body.local_get(1).i32_wrap_i64();
        loadHandle(body, 0, array.object);
        body.struct_get(array.object, ARRAY_LENGTH_FIELD).i32_lt_u()
            .if_(BlockType::Empty).i32_const(0).return_().end();
        body.local_get(0).local_get(1).i32_wrap_i64().call(grow);
    });
}

fn defineRecordTypes(
    functions: &mut Functions,
    records: &[Vec<Field>],
    arrays: &mut BTreeMap<ArrayElement, ArrayTypes>,
    recordArrays: impl IntoIterator<Item = ArrayElement>,
    string: Option<StringTypes>,
) -> Vec<u32> {
    let first = functions.types.len();
    let indices = (first..first + records.len() as u32).collect::<Vec<_>>();
    let mut arraySubtypes = Vec::new();
    let mut next = first + records.len() as u32;
    for element in recordArrays {
        let target = match element {
            ArrayElement::Record(record) => indices[record],
            ArrayElement::Nested { .. } => arrays[&element.inner().expect("nested array element")].object,
            _ => unreachable!("only recursively typed elements belong to the record group"),
        };
        let data = next;
        let object = next + 1;
        next += 2;
        arrays.insert(element, ArrayTypes { data, object });
        arraySubtypes.push(finalSubtype(CompositeInnerType::Array(ArrayType(FieldType {
            element_type: StorageType::Val(reference(target, true)),
            mutable: true,
        }))));
        arraySubtypes.push(finalSubtype(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                FieldType { element_type: StorageType::Val(ValType::I32), mutable: false },
                FieldType { element_type: StorageType::Val(reference(data, false)), mutable: false },
            FieldType { element_type: StorageType::Val(ValType::I32), mutable: true },
            FieldType { element_type: StorageType::I8, mutable: true },
            ]),
        })));
    }
    let mut types: Vec<_> = records
        .iter()
        .map(|fields| {
            let id = FieldType { element_type: StorageType::Val(ValType::I32), mutable: true };
            let fields = std::iter::once(id)
                .chain(fields.iter().map(|field| FieldType {
                    element_type: match field {
                        Field::Scalar(scalar) => scalar.storage(),
                        Field::Array(element) => StorageType::Val(reference(arrays[element].object, true)),
                        Field::String => {
                            StorageType::Val(reference(string.expect("string field type defined").object, true))
                        }
                        Field::Record(record) => StorageType::Val(reference(indices[*record], true)),
                        Field::AnyRecord => StorageType::Val(ValType::Ref(RefType::EQREF)),
                    },
                    mutable: true,
                }))
                .collect();
            finalSubtype(CompositeInnerType::Struct(StructType { fields }))
        })
        .collect();
    types.extend(arraySubtypes);
    if types.is_empty() { return indices; }
    // The section counts a recursion group as one entry; every member takes a type index.
    functions.extraTypeIndices = types.len() as u32 - 1;
    functions.types.ty().rec(types);
    indices
}

fn finalSubtype(inner: CompositeInnerType) -> SubType {
    SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType { inner, shared: false, descriptor: None, describes: None },
    }
}

fn emitRecordHelpers(
    functions: &mut Functions,
    record: usize,
    recordType: u32,
    fields: &[Field],
    arrays: &BTreeMap<ArrayElement, ArrayTypes>,
    string: Option<StringTypes>,
    records: &[u32],
) {
    use ValType::I32;

    functions.defineWithLocals(
        Some(&RecordOp::Alloc.symbol(record)),
        [],
        [I32],
        [reference(recordType, true)],
        |body| {
            body.struct_new_default(recordType).local_tee(0);
            body.global_get(NEXT_HANDLE).struct_set(recordType, HANDLE_ID_FIELD);
            body.local_get(0).ref_as_non_null().call(REMEMBER);
        },
    );
    for (index, field) in fields.iter().copied().enumerate() {
        let fieldIndex = index as u32 + 1;
        if field == Field::AnyRecord {
            let eqref = ValType::Ref(RefType::EQREF);
            functions.defineWithLocals(Some(&RecordOp::Get(index).symbol(record)), [I32], [I32], [eqref], |body| {
                loadHandle(body, 0, recordType);
                body.struct_get(recordType, fieldIndex).local_tee(1).ref_is_null();
                body.if_(BlockType::Result(I32)).i32_const(0).else_();
                body.local_get(1).i32_const(0).call(crate::arena::INTERN).end();
            });
            functions.define(Some(&RecordOp::Set(index).symbol(record)), [I32, I32], [], |body| {
                loadHandle(body, 0, recordType);
                body.local_get(1).i32_eqz().if_(BlockType::Result(eqref));
                body.ref_null(RefType::EQREF.heap_type).else_();
                body.local_get(1).call(crate::arena::LOOKUP).end();
                body.struct_set(recordType, fieldIndex);
            });
            continue;
        }
        let (abiType, referenceType) = match field {
            Field::Scalar(scalar) => (scalar.wasm(), None),
            Field::Array(element) => (I32, Some(arrays[&element].object)),
            Field::String => (I32, Some(string.expect("string field type defined").object)),
            Field::Record(record) => (I32, Some(records[record])),
            Field::AnyRecord => unreachable!(),
        };
        if let Some(object) = referenceType {
            let fieldRef = reference(object, true);
            functions.defineWithLocals(Some(&RecordOp::Get(index).symbol(record)), [I32], [I32], [fieldRef], |body| {
                loadHandle(body, 0, recordType);
                body.struct_get(recordType, fieldIndex);
                nullableReferenceToHandle(body, 1, object);
            });
            functions.define(Some(&RecordOp::Set(index).symbol(record)), [I32, I32], [], |body| {
                loadHandle(body, 0, recordType);
                nullableHandleToReference(body, 1, object);
                body.struct_set(recordType, fieldIndex);
            });
        } else {
            functions.define(Some(&RecordOp::Get(index).symbol(record)), [I32], [abiType], |body| {
                loadHandle(body, 0, recordType);
                if matches!(field, Field::Scalar(Scalar::I8 | Scalar::I16)) {
                    body.struct_get_u(recordType, fieldIndex);
                } else {
                    body.struct_get(recordType, fieldIndex);
                }
            });
            functions.define(Some(&RecordOp::Set(index).symbol(record)), [I32, abiType], [], |body| {
                loadHandle(body, 0, recordType);
                body.local_get(1);
                body.struct_set(recordType, fieldIndex);
            });
        }
    }
}

fn nullableReferenceToHandle(body: &mut InstructionSink<'_>, local: u32, object: u32) {
    body.local_tee(local).ref_is_null();
    body.if_(BlockType::Result(ValType::I32)).i32_const(0).else_();
    body.local_get(local).local_get(local).struct_get(object, HANDLE_ID_FIELD).call(crate::arena::INTERN).end();
}

fn nullableHandleToReference(body: &mut InstructionSink<'_>, local: u32, object: u32) {
    body.local_get(local).i32_eqz().if_(BlockType::Result(reference(object, true)));
    body.ref_null(HeapType::Concrete(object)).else_();
    loadHandle(body, local, object);
    body.end();
}

fn loadHandle(body: &mut InstructionSink<'_>, local: u32, typeIndex: u32) {
    body.local_get(local);
    loadHandleFromStack(body, typeIndex);
}

pub(crate) fn loadHandleFromStack(body: &mut InstructionSink<'_>, typeIndex: u32) {
    body.call(crate::arena::LOOKUP).ref_cast_non_null(HeapType::Concrete(typeIndex));
}

fn loadArray(body: &mut InstructionSink<'_>, arrayObject: u32) {
    body.local_get(0);
    loadArrayFromStack(body, arrayObject);
}

pub(crate) fn loadArrayFromStack(body: &mut InstructionSink<'_>, arrayObject: u32) {
    loadHandleFromStack(body, arrayObject);
    body.struct_get(arrayObject, ARRAY_DATA_FIELD);
}

#[derive(Default)]
pub(crate) struct Functions {
    pub(crate) types: TypeSection,
    extraTypeIndices: u32,
    declarations: FunctionSection,
    exports: ExportSection,
    code: CodeSection,
}

impl Functions {
    pub(crate) fn define<const P: usize, const R: usize>(
        &mut self,
        name: Option<&str>,
        params: [ValType; P],
        results: [ValType; R],
        emit: impl FnOnce(&mut InstructionSink<'_>),
    ) {
        self.defineWithLocals(name, params, results, [], emit);
    }

    pub(crate) fn defineWithLocals<const P: usize, const R: usize, const L: usize>(
        &mut self,
        name: Option<&str>,
        params: [ValType; P],
        results: [ValType; R],
        locals: [ValType; L],
        emit: impl FnOnce(&mut InstructionSink<'_>),
    ) {
        let typeIndex = self.types.len() + self.extraTypeIndices;
        self.types.ty().function(params, results);
        if let Some(name) = name {
            self.exports.export(name, ExportKind::Func, self.declarations.len());
        }
        self.declarations.function(typeIndex);
        let mut function = Function::new_with_locals_types(locals);
        let mut body = function.instructions();
        emit(&mut body);
        body.end();
        self.code.function(&function);
    }
}
