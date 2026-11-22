//! Event lists live in the GC owner. Their entries are the shared managed closure records.

use super::*;
use cranelift_codegen::ir::{
    AbiParam, Signature,
    types::{I32 as CL_I32, I64 as CL_I64},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Event {
    pub owner: usize,
    pub field: usize,
    pub source_offset: u32,
    pub capacity: u32,
    pub handler: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Operation {
    Subscribe,
    Unsubscribe,
    Length,
    Handler,
}

impl Operation {
    pub const ALL: [Self; 4] = [Self::Subscribe, Self::Unsubscribe, Self::Length, Self::Handler];

    pub fn native(name: &str) -> Option<Self> {
        Some(match name {
            "event_subscribe" => Self::Subscribe,
            "event_unsubscribe_first" => Self::Unsubscribe,
            "event_len" => Self::Length,
            "event_get_handler" => Self::Handler,
            _ => return None,
        })
    }

    pub fn matches(self, signature: &Signature) -> bool {
        let params = match self {
            Self::Subscribe => &[CL_I64, CL_I64, CL_I64][..],
            Self::Unsubscribe => &[CL_I64, CL_I64],
            Self::Length => &[CL_I64],
            Self::Handler => &[CL_I64, CL_I32],
        };
        signature.params.iter().cloned().eq(params.iter().copied().map(AbiParam::new))
            && signature.returns == [AbiParam::new(CL_I64)]
    }

    pub fn name(self, event: Event) -> String {
        let operation = match self {
            Self::Subscribe => "subscribe",
            Self::Unsubscribe => "unsubscribe",
            Self::Length => "length",
            Self::Handler => "handler",
        };
        format!("__beskid_browser_event_{operation}_{}_{}", event.owner, event.field)
    }
}

pub(super) fn emit(
    functions: &mut Functions,
    event: Event,
    records: &[u32],
    arrays: &BTreeMap<ArrayElement, ArrayTypes>,
) {
    use ValType::{I32, I64};
    let owner = records[event.owner];
    let handler = records[event.handler];
    let list = arrays[&ArrayElement::Record(event.handler)];
    let field = event.field as u32 + 1;
    let list_ref = reference(list.object, true);
    let data_ref = reference(list.data, true);
    let handler_ref = reference(handler, true);
    let load = |b: &mut InstructionSink<'_>| {
        loadHandle(b, 0, owner);
        b.struct_get(owner, field);
    };

    functions.defineWithLocals(Some(&Operation::Length.name(event)), [I32], [I64], [list_ref], |body| {
        load(body);
        body.local_tee(1).ref_is_null().if_(BlockType::Result(I64));
        body.i64_const(0).else_();
        body.local_get(1).struct_get(list.object, ARRAY_LENGTH_FIELD).i64_extend_i32_u().end();
    });
    functions.defineWithLocals(
        Some(&Operation::Handler.name(event)),
        [I32, I32],
        [I32],
        [list_ref, handler_ref],
        |body| {
            load(body);
            body.local_tee(2).ref_is_null().if_(BlockType::Empty);
            body.i32_const(0).return_().end();
            body.local_get(1).local_get(2).struct_get(list.object, ARRAY_LENGTH_FIELD);
            body.i32_ge_u().if_(BlockType::Empty).i32_const(0).return_().end();
            body.local_get(2).struct_get(list.object, ARRAY_DATA_FIELD).local_get(1).array_get(list.data);
            nullableReferenceToHandle(body, 3, handler);
        },
    );
    functions.defineWithLocals(
        Some(&Operation::Subscribe.name(event)),
        [I32, I32],
        [I64],
        [reference(owner, true), list_ref, I32],
        |body| {
            body.local_get(1).i32_eqz().if_(BlockType::Empty).i64_const(0).return_().end();
            loadHandle(body, 0, owner);
            body.local_tee(2).struct_get(owner, field).local_tee(3).ref_is_null().if_(BlockType::Empty);
            body.i32_const(0); // The hidden list has no externally visible arena handle.
            body.i32_const(event.capacity as i32).array_new_default(list.data);
            body.i32_const(0).i32_const(0).struct_new(list.object).local_set(3);
            body.local_get(2).local_get(3).struct_set(owner, field).end();
            body.local_get(3).struct_get(list.object, ARRAY_LENGTH_FIELD).local_tee(4);
            body.i32_const(event.capacity as i32).i32_ge_u().if_(BlockType::Empty);
            body.local_get(4).i64_extend_i32_u().return_().end();
            body.local_get(3).struct_get(list.object, ARRAY_DATA_FIELD).local_get(4);
            loadHandle(body, 1, handler);
            body.array_set(list.data);
            body.local_get(3).local_get(4).i32_const(1).i32_add().local_tee(4);
            body.struct_set(list.object, ARRAY_LENGTH_FIELD);
            body.local_get(4).i64_extend_i32_u();
        },
    );
    functions.defineWithLocals(
        Some(&Operation::Unsubscribe.name(event)),
        [I32, I32],
        [I64],
        [list_ref, data_ref, I32, I32, handler_ref],
        |body| {
            body.local_get(1).i32_eqz().if_(BlockType::Empty).i64_const(0).return_().end();
            load(body);
            body.local_tee(2).ref_is_null().if_(BlockType::Empty).i64_const(0).return_().end();
            body.local_get(2).struct_get(list.object, ARRAY_DATA_FIELD).local_set(3);
            body.local_get(2).struct_get(list.object, ARRAY_LENGTH_FIELD).local_set(4);
            loadHandle(body, 1, handler);
            body.local_set(6);
            body.block(BlockType::Empty).loop_(BlockType::Empty);
            body.local_get(5).local_get(4).i32_ge_u().br_if(1);
            body.local_get(3).local_get(5).array_get(list.data).local_get(6).ref_eq().if_(BlockType::Empty);
            body.local_get(3).local_get(5).local_get(3).local_get(5).i32_const(1).i32_add();
            body.local_get(4).local_get(5).i32_sub().i32_const(1).i32_sub().array_copy(list.data, list.data);
            body.local_get(4).i32_const(1).i32_sub().local_set(4);
            body.local_get(3).local_get(4).ref_null(HeapType::Concrete(handler)).array_set(list.data);
            body.local_get(2).local_get(4).struct_set(list.object, ARRAY_LENGTH_FIELD);
            body.i64_const(1).return_().end();
            body.local_get(5).i32_const(1).i32_add().local_set(5).br(0).end().end();
            body.i64_const(0);
        },
    );
}
