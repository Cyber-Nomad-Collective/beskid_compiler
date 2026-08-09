use std::collections::HashSet;
use std::sync::Arc;

use cranelift_codegen::ir::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayLayout {
    pub(crate) element_type: Type,
    pub(crate) stride: u32,
    pub(crate) length: u32,
    align_shift: u8,
}

impl ArrayLayout {
    pub const fn new(element_type: Type, stride: u32, length: u32, align_shift: u8) -> Self {
        Self { element_type, stride, length, align_shift }
    }

    pub(crate) fn byte_size(self) -> Option<u32> {
        self.stride.checked_mul(self.length)
    }

    pub(crate) fn is_valid(self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        self.element_type.bytes() > 0
            && self.stride >= self.element_type.bytes()
            && self.stride.is_multiple_of(alignment)
            && self.byte_size().is_some_and(|size| size <= i32::MAX as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    pub value_type: Type,
    pub offset: u32,
}

impl FieldLayout {
    pub const fn new(value_type: Type, offset: u32) -> Self {
        Self { value_type, offset }
    }
}

fn aggregate_field_is_valid(size: u32, alignment: u32, field: FieldLayout) -> bool {
    let field_size = field.value_type.bytes();
    let Some(end) = field.offset.checked_add(field_size) else {
        return false;
    };
    let field_alignment = field_size.next_power_of_two().min(alignment);
    field_size > 0 && end <= size && field.offset.is_multiple_of(field_alignment)
}

fn aggregate_fields_overlap(left: FieldLayout, right: FieldLayout) -> bool {
    let left_end = left.offset + left.value_type.bytes();
    let right_end = right.offset + right.value_type.bytes();
    left.offset < right_end && right.offset < left_end
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructLayout {
    size: u32,
    align_shift: u8,
    pub(crate) fields: Vec<FieldLayout>,
}

/// Source-authorized static request used to allocate one aggregate literal through ABI-v5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedStructAllocation {
    pub allocation_request_symbol: Arc<str>,
}

/// Source-authorized static request used to allocate one typed array through ABI-v5.
///
/// The request owns the element pointer-map identity. ISLE must not reconstruct this from a
/// CLIF value type or an element size because that would lose persistent GC reachability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedArrayAllocation {
    pub allocation_request_symbol: Arc<str>,
}

impl StructLayout {
    pub fn new(size: u32, align_shift: u8, fields: Vec<FieldLayout>) -> Self {
        Self { size, align_shift, fields }
    }

    pub(crate) fn is_valid(&self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        if self.size == 0 || self.size > i32::MAX as u32 || !self.size.is_multiple_of(alignment) {
            return false;
        }
        for (index, field) in self.fields.iter().enumerate() {
            if !aggregate_field_is_valid(self.size, alignment, *field) {
                return false;
            }
            if self.fields[..index].iter().any(|other| aggregate_fields_overlap(*field, *other)) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumVariantLayout {
    pub discriminant: u64,
    pub payload: Option<FieldLayout>,
}

impl EnumVariantLayout {
    pub const fn new(discriminant: u64, payload: Option<FieldLayout>) -> Self {
        Self { discriminant, payload }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumLayout {
    pub size: u32,
    pub align_shift: u8,
    pub tag: FieldLayout,
    pub variants: Vec<EnumVariantLayout>,
}

impl EnumLayout {
    pub fn new(size: u32, align_shift: u8, tag: FieldLayout, variants: Vec<EnumVariantLayout>) -> Self {
        Self { size, align_shift, tag, variants }
    }

    pub fn is_valid(&self) -> bool {
        let Some(alignment) = 1_u32.checked_shl(u32::from(self.align_shift)) else {
            return false;
        };
        if self.size == 0
            || self.size > i32::MAX as u32
            || !self.size.is_multiple_of(alignment)
            || !self.tag.value_type.is_int()
            || !aggregate_field_is_valid(self.size, alignment, self.tag)
            || self.variants.is_empty()
        {
            return false;
        }
        let tag_bits = self.tag.value_type.bits();
        if tag_bits > 64 {
            return false;
        }
        let mut discriminants = HashSet::with_capacity(self.variants.len());
        self.variants.iter().all(|variant| {
            let discriminant_fits = tag_bits == 64 || variant.discriminant < (1_u64 << tag_bits);
            let payload_is_valid = variant.payload.is_none_or(|payload| {
                aggregate_field_is_valid(self.size, alignment, payload) && !aggregate_fields_overlap(self.tag, payload)
            });
            discriminant_fits && payload_is_valid && discriminants.insert(variant.discriminant)
        })
    }
}
