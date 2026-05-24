//! Dynamic typing runtime: cells, shape tables, and fallback mapping (v0.3).

mod cell;
mod fallback;
mod table;

pub use cell::DynamicCell;
pub use fallback::{
    DYNAMIC_ERR_INCOMPATIBLE, DYNAMIC_ERR_NULL_PAYLOAD, DYNAMIC_ERR_UNKNOWN_DST_SHAPE,
    DYNAMIC_ERR_UNKNOWN_SRC_SHAPE, DYNAMIC_OK, map_dynamic_fallback, map_objects_aot,
};
pub use table::{FieldStep, ShapeEntry, mapping_steps, register_mapping, register_shape,
                reset_tables_for_test, shape_object_size};
