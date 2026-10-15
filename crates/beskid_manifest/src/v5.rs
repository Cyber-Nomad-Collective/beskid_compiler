//! ABI-v5 runtime manifest parsing and deterministic multi-target generation.

mod artifacts;
mod model;
mod parsing;
mod render;
mod validation;

pub use artifacts::{generate_v5_artifacts, runtime_layout_source, write_v5_artifacts};
pub use model::{GeneratedV5Artifacts, RuntimeManifestV5};
pub use parsing::load_v5_manifest_source;

/// One compiler-emitted checked clone of an exact canonical runtime source function.
///
/// `source_path` is the logical path inside the canonical runtime package
/// (`runtime/beskid`). The clone keeps the source function's ABI signature and is
/// exported under `export`; it grants source callers no recovery authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedRuntimeClone {
    pub source_path: &'static str,
    pub function: &'static str,
    pub export: &'static str,
}

/// Sole inventory of checked runtime clones. Codegen emits exactly these exports,
/// canonical runtime preparation admits exactly these, and the manifest source-authority
/// gate derives their ABI from the named source function.
pub const CHECKED_RUNTIME_CLONES: &[CheckedRuntimeClone] = &[
    CheckedRuntimeClone {
        source_path: "src/Runtime/Glue/OwnerRecord.bd",
        function: "GlueRecordLeaf",
        export: "beskid_glue_v1_checked_record_leaf",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Glue/OwnerRecord.bd",
        function: "GlueRecordLink",
        export: "beskid_glue_v1_checked_record_link",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Data/Utf8ViewRecord.bd",
        function: "Utf8RecordConstruct",
        export: "beskid_rt_v5_checked_utf8_record_construct",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Dynamic/Records.bd",
        function: "DynamicErasedConstructV1",
        export: "beskid_dynamic_v1_checked_erased_construct",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Dynamic/Records.bd",
        function: "DynamicErasedReadV1",
        export: "beskid_dynamic_v1_checked_erased_read",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Dynamic/Dynamic.bd",
        function: "DynamicCreateResultFactoryV1",
        export: "beskid_dynamic_v1_checked_create_result_factory",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Dynamic/Dynamic.bd",
        function: "DynamicCastResultFactoryV1",
        export: "beskid_dynamic_v1_checked_cast_result_factory",
    },
    CheckedRuntimeClone {
        source_path: "src/Runtime/Dynamic/Dynamic.bd",
        function: "DynamicMapResultFactoryV1",
        export: "beskid_dynamic_v1_checked_map_result_factory",
    },
];

/// The checked clone exported as `symbol`, if any.
pub fn checked_runtime_clone(symbol: &str) -> Option<&'static CheckedRuntimeClone> {
    CHECKED_RUNTIME_CLONES.iter().find(|clone| clone.export == symbol)
}

/// Closed independently versioned Glue owner transport schema.
pub fn glue_owner_v1_contract(symbol: &str) -> Option<(&'static [&'static str], &'static str)> {
    match symbol {
        "beskid_glue_v1_owner_bind_opaque_domains" => Some((&["u64", "u64", "pointer", "usize"], "i32")),
        "beskid_glue_v1_owner_opaque_create" => Some((&["u64", "pointer", "pointer", "pointer", "pointer"], "i32")),
        "beskid_glue_v1_owner_opaque_borrow_begin" => Some((&["u64", "pointer", "u64", "pointer", "pointer"], "i32")),
        "beskid_glue_v1_owner_opaque_borrow_end" => Some((&["u64", "pointer", "u64"], "i32")),
        "beskid_glue_v1_owner_opaque_release" => Some((&["u64", "pointer", "u64"], "i32")),
        "beskid_glue_v1_owner_validate_opaque" => Some((&["u64", "pointer", "u64"], "i32")),
        "beskid_glue_v1_owner_release_consumer_opaque" => Some((&["u64", "pointer", "u64"], "i32")),
        "beskid_glue_v1_record_begin_close" => Some((&["pointer"], "usize")),
        "beskid_glue_v1_next_identity" => Some((&[], "u64")),
        "beskid_glue_v1_host_open" => Some((&["pointer"], "i32")),
        "beskid_glue_v1_host_close" => Some((&["pointer"], "i32")),
        "beskid_glue_v1_owner_open_library" => Some((&["u64", "pointer"], "i32")),
        "beskid_glue_v1_owner_close_library" => Some((&["u64", "u64"], "i32")),
        "beskid_glue_v1_owner_release_token" => Some((&["u64", "u64"], "i32")),
        "beskid_glue_v1_owner_validate_binding" => Some((&["u64", "u64"], "i32")),
        "beskid_glue_v1_owner_bind_shapes" => Some((&["u64", "u64", "pointer", "usize"], "i32")),
        "beskid_glue_v1_owner_bind_image_closure" => Some((&["u64", "u64", "pointer", "usize"], "i32")),
        "beskid_glue_v1_owner_copy" => Some((&["u64", "u64", "u64", "u64", "pointer", "usize", "pointer"], "i32")),
        "beskid_glue_v1_owner_release" => Some((&["u64", "u64", "u64", "u64", "u64"], "i32")),
        "beskid_glue_v1_input_utf8" | "beskid_glue_v1_input_bytes" => {
            Some((&["pointer", "usize", "pointer", "pointer"], "i32"))
        }
        "beskid_glue_v1_result_utf8" | "beskid_glue_v1_result_bytes" => {
            Some((&["u64", "u64", "pointer", "pointer"], "i32"))
        }
        "beskid_glue_v1_owner_shutdown" => Some((&[], "u8")),
        "beskid_glue_v1_record_leaf" | "beskid_glue_v1_checked_record_leaf" => {
            Some((&["pointer", "usize", "usize", "u64", "u64", "u64", "u64", "u64", "u64", "u64"], "pointer"))
        }
        "beskid_glue_v1_record_link" | "beskid_glue_v1_checked_record_link" => Some((
            &["pointer", "pointer", "usize", "usize", "u64", "u64", "u64", "u64", "u64", "u64", "u64"],
            "pointer",
        )),
        "beskid_glue_v1_record_next_count" => Some((&["pointer"], "usize")),
        "beskid_glue_v1_record_previous" => Some((&["pointer"], "pointer")),
        "beskid_glue_v1_record_tag" => Some((&["pointer", "usize"], "u64")),
        "beskid_glue_v1_record_bind_shapes" => Some((&["pointer", "pointer"], "usize")),
        "beskid_glue_v1_record_has_shape" => Some((&["pointer", "u64"], "usize")),
        "beskid_glue_v1_record_release" => Some((&["pointer", "pointer"], "usize")),
        "beskid_glue_v1_record_payload" => Some((&["pointer"], "pointer")),
        "beskid_glue_v1_root_load" => Some((&[], "pointer")),
        "beskid_glue_v1_root_publish" => Some((&["pointer"], "u8")),
        "beskid_glue_v1_root_clear" => Some((&[], "void")),
        "beskid_glue_v1_runtime_generation" => Some((&[], "u64")),
        _ => None,
    }
}

/// Closed Dynamic V1 transport; constructor parameters retain typed source authority.
pub fn dynamic_v1_contract(symbol: &str) -> Option<(&'static [&'static str], &'static str)> {
    match symbol {
        "beskid_dynamic_v1_erased_construct" | "beskid_dynamic_v1_checked_erased_construct" => {
            Some((&["pointer", "pointer"], "pointer"))
        }
        "beskid_dynamic_v1_checked_erased_read" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_checked_create_result_factory"
        | "beskid_dynamic_v1_checked_cast_result_factory"
        | "beskid_dynamic_v1_checked_map_result_factory" => {
            Some((&["pointer", "u64", "pointer"], "pointer"))
        }
        "beskid_dynamic_v1_checked_create_result_dispatch"
        | "beskid_dynamic_v1_checked_cast_result_dispatch"
        | "beskid_dynamic_v1_checked_map_result_dispatch" => {
            Some((&["pointer", "u64"], "pointer"))
        }
        "beskid_dynamic_v1_erased_read" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_erased_cell_descriptor" => Some((&[], "pointer")),
        "beskid_dynamic_v1_create_result" | "beskid_dynamic_v1_cast_result" | "beskid_dynamic_v1_map_result" => {
            Some((&["pointer", "u64"], "pointer"))
        }
        "beskid_dynamic_v1_mapping_construct" => {
            Some((&["pointer", "pointer", "pointer", "pointer", "u64", "u64", "u64"], "pointer"))
        }
        "beskid_dynamic_v1_registry_append_mapping" => Some((&["pointer", "pointer"], "void")),
        "beskid_dynamic_v1_registry_mapping_count" => Some((&["pointer"], "usize")),
        "beskid_dynamic_v1_registry_mapping_at" => Some((&["pointer", "usize"], "pointer")),
        "beskid_dynamic_v1_mapping_source" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_mapping_destination" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_mapping_signature" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_mapping_transform" => Some((&["pointer"], "pointer")),
        "beskid_dynamic_v1_mapping_tag" => Some((&["pointer", "usize"], "u64")),
        "beskid_dynamic_v1_register_mapping" => Some((&["pointer", "pointer"], "i32")),
        "beskid_dynamic_v1_map" => Some((&["pointer", "u64", "pointer"], "i32")),
        "beskid_dynamic_v1_create_owned" | "beskid_dynamic_v1_cast_owned" | "beskid_dynamic_v1_map_owned" => {
            Some((&["pointer", "u64", "pointer"], "pointer"))
        }
        "beskid_dynamic_v1_register_shape" => Some((&["pointer", "pointer"], "i32")),
        "beskid_dynamic_v1_create" | "beskid_dynamic_v1_cast" => Some((&["pointer", "u64", "pointer"], "i32")),
        "beskid_dynamic_v1_shutdown" => Some((&[], "u8")),
        "beskid_dynamic_v1_descriptor_valid" => Some((&["pointer", "u8"], "u8")),
        "beskid_dynamic_v1_root_load" => Some((&[], "pointer")),
        "beskid_dynamic_v1_root_publish" => Some((&["pointer"], "u8")),
        "beskid_dynamic_v1_root_clear" => Some((&[], "void")),
        "beskid_dynamic_v1_registry_construct" => Some((&["u64", "u64", "u64"], "pointer")),
        "beskid_dynamic_v1_shape_construct" => Some((
            &[
                "pointer", "pointer", "pointer", "pointer", "pointer", "pointer", "pointer", "pointer", "u64", "u64",
                "u64",
            ],
            "pointer",
        )),
        "beskid_dynamic_v1_registry_append_shape" => Some((&["pointer", "pointer"], "void")),
        "beskid_dynamic_v1_registry_shape_count" => Some((&["pointer"], "usize")),
        "beskid_dynamic_v1_registry_shape_at" => Some((&["pointer", "usize"], "pointer")),
        "beskid_dynamic_v1_registry_tag" => Some((&["pointer", "usize"], "u64")),
        "beskid_dynamic_v1_registry_close" => Some((&["pointer"], "void")),
        "beskid_dynamic_v1_shape_pointer" => Some((&["pointer", "usize"], "pointer")),
        "beskid_dynamic_v1_shape_tag" => Some((&["pointer", "usize"], "u64")),
        "beskid_dynamic_v1_signature_digest"
        | "beskid_dynamic_v1_shape_signature"
        | "beskid_dynamic_v1_shape_digest"
        | "beskid_dynamic_v1_shape_source"
        | "beskid_dynamic_v1_shape_owner" => Some((&["pointer"], "pointer")),
        _ => None,
    }
}

pub fn independently_versioned_runtime_contract(symbol: &str) -> Option<(&'static [&'static str], &'static str)> {
    glue_owner_v1_contract(symbol).or_else(|| dynamic_v1_contract(symbol))
}
