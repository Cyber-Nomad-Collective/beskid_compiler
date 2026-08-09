use std::path::PathBuf;
use std::sync::Arc;

use crate::resolve::ItemId;
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, UnitTypeSurface, merge_unit_surfaces};

#[test]
fn merge_prefers_entry_surface_on_conflict() {
    let item = ItemId(1);
    let i32 = TypeId(0);
    let i64 = TypeId(1);

    let mut dep = UnitTypeSurface::default();
    dep.function_signatures.insert(item, FunctionSignature { params: vec![i32], return_type: i32 });

    let mut entry = UnitTypeSurface::default();
    entry.function_signatures.insert(item, FunctionSignature { params: vec![i64], return_type: i64 });

    let merged = merge_unit_surfaces(std::iter::once((PathBuf::from("dep.bd"), Arc::new(dep))), Arc::new(entry));
    assert_eq!(merged.function_signatures.get(&item), Some(&FunctionSignature { params: vec![i64], return_type: i64 }));
}
