//! Compile-time and runtime shape tables for object-to-object mapping.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One field copy step in a shape-to-shape mapping (deterministic declaration order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldStep {
    pub src_offset: u32,
    pub dst_offset: u32,
    pub size: u32,
}

/// Registered object shape (size + optional outbound mappings).
#[derive(Debug, Clone)]
pub struct ShapeEntry {
    pub shape_id: u32,
    pub object_size: usize,
}

type ShapeMappingTable = HashMap<(u32, u32), Vec<FieldStep>>;

static SHAPES: OnceLock<Mutex<HashMap<u32, ShapeEntry>>> = OnceLock::new();
static MAPPINGS: OnceLock<Mutex<ShapeMappingTable>> = OnceLock::new();

fn shapes() -> &'static Mutex<HashMap<u32, ShapeEntry>> {
    SHAPES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mappings() -> &'static Mutex<ShapeMappingTable> {
    MAPPINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register or replace a shape description (tests and host init).
pub fn register_shape(shape_id: u32, object_size: usize) {
    shapes().lock().expect("shape table lock").insert(shape_id, ShapeEntry { shape_id, object_size });
}

/// Register a deterministic field mapping from `src_shape` to `dst_shape`.
pub fn register_mapping(src_shape: u32, dst_shape: u32, steps: Vec<FieldStep>) {
    mappings().lock().expect("mapping table lock").insert((src_shape, dst_shape), steps);
}

pub fn shape_object_size(shape_id: u32) -> Option<usize> {
    shapes().lock().expect("shape table lock").get(&shape_id).map(|entry| entry.object_size)
}

pub fn mapping_steps(src_shape: u32, dst_shape: u32) -> Option<Vec<FieldStep>> {
    mappings().lock().expect("mapping table lock").get(&(src_shape, dst_shape)).cloned()
}

/// Reset tables (unit tests only).
#[doc(hidden)]
pub fn reset_tables_for_test() {
    shapes().lock().expect("shape table lock").clear();
    mappings().lock().expect("mapping table lock").clear();
}
