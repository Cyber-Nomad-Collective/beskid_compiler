//! Private, generation-issued source call and layout authority for the CABI2 codec.
//! No deserialization or public constructors: only the canonical producer issues this plan.
use beskid_queries::SemanticTypeId;

#[derive(serde::Serialize)]
pub(crate) struct NativeAdapterPlan {
    pub syntax_types: Vec<NativeSyntaxType>,
    pub pointer_bytes: u8,
    pub discriminator_symbol: String,
    pub types: Vec<NativeAdapterType>,
    pub constructors: Vec<NativeConstructor>,
    pub entries: Vec<NativeEntryPlan>,
    pub callbacks: Vec<NativeCallbackPlan>,
    pub runtime: NativeRuntimeHooks,
}
#[derive(serde::Serialize)]
pub(crate) struct NativeAdapterType {
    pub id: u32,
    pub name: String,
    pub kind: NativeAdapterKind,
}
#[derive(serde::Serialize)]
pub(crate) enum NativeAdapterKind {
    Scalar(SemanticTypeId),
    Array {
        element: u32,
        stride: u64,
        data_offset: u64,
        length_offset: u64,
        request_getter: String,
        request_bytes: u64,
        count_offset: u64,
    },
    Record {
        fields: Vec<NativeAdapterField>,
    },
    Enum {
        tag_offset: u64,
        tag_bytes: u8,
        variants: Vec<NativeAdapterVariant>,
    },
}
#[derive(serde::Serialize)]
pub(crate) struct NativeAdapterField {
    pub name: String,
    pub type_id: u32,
    pub offset: u64,
}
#[derive(serde::Serialize)]
pub(crate) struct NativeAdapterVariant {
    pub name: String,
    pub tag: u64,
    pub fields: Vec<NativeAdapterField>,
}
#[derive(serde::Serialize)]
pub(crate) struct NativeConstructor {
    pub symbol: String,
    pub result_type: u32,
    pub variant: Option<u32>,
    pub parameters: Vec<u32>,
    pub bindings: Vec<NativeConstructorArgument>,
}
#[derive(Clone, serde::Serialize)]
pub(crate) enum NativeConstructorArgument {
    Field(u32),
    /// Exact source factory accepts the sole element of an array-valued field.
    ArrayElement {
        field: u32,
        index: u32,
    },
}
#[derive(serde::Serialize)]
pub(crate) struct NativeEntryPlan {
    pub symbol: String,
    pub method_symbol: String,
    pub family: String,
    pub receiver_type: u32,
    pub factory_symbol: String,
    pub factory_receiver: NativeReceiverBootstrap,
    pub factory_request_type: u32,
    pub request_type: u32,
    pub result_type: u32,
}
#[derive(serde::Serialize)]
pub(crate) enum NativeReceiverBootstrap {
    FactoryRequest {
        type_id: u32,
    },
    /// A canonical source constructor, including recursively initialized factory receivers.
    SourceCall {
        symbol: String,
        arguments: Vec<NativeReceiverBootstrap>,
    },
    /// Source-issued allocation metadata for a proven field-free concrete factory.
    EmptyRecord {
        request_getter: String,
    },
}
#[derive(serde::Serialize)]
pub(crate) struct NativeCallbackPlan {
    pub symbol: String,
    pub operation: String,
    pub parameters: Vec<u32>,
    pub result_type: u32,
}
#[derive(serde::Serialize)]
pub(crate) struct NativeRuntimeHooks {
    pub string_construct: String,
    pub string_length: String,
    pub string_data_offset: u64,
    pub root: String,
    pub resolve_root: String,
    pub unroot: String,
    pub array_allocate_rooted: String,
    pub construction_finish: String,
    pub record_allocate: String,
    pub write_barrier: String,
    pub array_write_barrier: String,
}

/// Version 2 transport uses opaque invocation-local value handles, never managed pointers.
/// Header: u32 version, u32 bytes, u64 generation, u64 invocation,
/// void* context, callbacks*, u64 request, u64 factory_request.
/// Callback slots (all return i32 status; zero success):
/// kind(context,u64,u32*), scalar(context,u64,u64*), text(context,u64,const u8**,usize*),
/// count(context,u64,usize*), child(context,u64,usize,u64*), variant(context,u64,u32*),
/// new_scalar(context,u32,u64,u64*), new_text(context,const u8*,usize,u64*),
/// new_sequence(context,u32,const u64*,usize,u64*), new_variant(context,u32,u32,const u64*,usize,u64*),
/// service(context,const u8*,usize,const u64*,usize,u64*).
/// Entry: i32 entry(const Header*,u64* result). All out values zeroed on failure.
pub(crate) const NATIVE_ADAPTER_ABI_VERSION: u32 = 2;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeValueKind {
    Unit = 0,
    Bool = 1,
    Signed = 2,
    Unsigned = 3,
    FloatBits = 4,
    String = 5,
    Sequence = 6,
    Record = 7,
    Variant = 8,
}

#[derive(serde::Serialize)]
pub(crate) struct NativeSyntaxType {
    pub type_id: u32,
    pub schema_type: String,
}
