//! `move` and `exchange` lowering (language-meta/memory-model/memory-and-references,
//! "Destructive read of a field"; execution/runtime/memory-and-gc-runtime-contract, "Hybrid
//! write barrier"): `move x.f` reads the field and stores the canonical empty value
//! (`Option::None`, the empty array, the empty list) into it behind the guarded barrier;
//! `exchange(place, v)` reads the place and stores `v` behind the guarded barrier. Each yields the
//! value it read. Since 0.6.7 `move x` of a name is written only at an isolate crossing (E1239
//! elsewhere), so its transfer lowering is covered by the crossing tests, not here.

use super::write_barrier_emission::{assert_guarded_stores, calls_to, lower_module};

const ARRAY_ALLOCATE: &str = "beskid_rt_v5_array_allocate_rooted";
const OBJECT_ALLOCATE: &str = "beskid_rt_v5_managed_object_allocate";

/// The value the function returns, and the instruction that defines it.
fn returned_definition(clif: &str) -> String {
    let returned = clif
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("return "))
        .unwrap_or_else(|| panic!("the function returns a value: {clif}"))
        .trim()
        .to_owned();
    clif.lines()
        .map(|line| line.split(';').next().unwrap_or(line).trim())
        .find(|line| line.starts_with(&format!("{returned} = ")))
        .unwrap_or_else(|| panic!("{returned} is defined: {clif}"))
        .to_owned()
}

/// The yielded value is the field (or element) read before the store.
fn assert_yields_loaded_value(clif: &str) {
    let definition = returned_definition(clif);
    assert!(definition.contains(" = load.i64 "), "the result is the value read from the place: {definition}\n{clif}");
}

#[test]
fn destructive_read_of_an_array_field_stores_the_empty_array_behind_the_barrier() {
    let take = &lower_module("type Holder { u8[] buf; } u8[] Take(mut Holder h) { return move h.buf; }", &["Take"])[0];
    assert_eq!(assert_guarded_stores(take), 1, "{take}");
    assert_eq!(calls_to(take, ARRAY_ALLOCATE), 1, "the empty array is allocated: {take}");
    assert!(take.contains("beskid_rt_v5_array_construction_finish"), "{take}");
    assert_yields_loaded_value(take);
}

#[test]
fn destructive_read_of_an_option_field_stores_none_behind_the_barrier() {
    let take = &lower_module(
        r#"
pub enum Option<T> { Some(T value), None }
type Slot { Option<string> item; }
Option<string> Take(mut Slot s) { return move s.item; }
"#,
        &["Take"],
    )[0];
    assert_eq!(assert_guarded_stores(take), 1, "{take}");
    assert_eq!(calls_to(take, OBJECT_ALLOCATE), 1, "the None variant is allocated: {take}");
    assert!(take.contains("__beskid_canonical_request_"), "the None object has its own request: {take}");
    assert_yields_loaded_value(take);
}

#[test]
fn destructive_read_of_a_list_field_stores_the_empty_list_behind_the_barrier() {
    let take = &lower_module(
        r#"
type List<T> { T[] storage; i64 count; }
type Bag { List<string> items; }
List<string> Take(mut Bag b) { return move b.items; }
"#,
        &["Take"],
    )[0];
    assert_eq!(assert_guarded_stores(take), 1, "only the store into the existing object is barriered: {take}");
    assert_eq!(calls_to(take, ARRAY_ALLOCATE), 1, "the empty storage is allocated: {take}");
    assert_eq!(calls_to(take, OBJECT_ALLOCATE), 1, "the empty list is allocated: {take}");
    assert_yields_loaded_value(take);
}

#[test]
fn exchange_of_a_field_stores_the_value_behind_the_barrier_and_yields_the_old_value() {
    let swap = &lower_module(
        "type Stage { u8[] buffer; } u8[] Swap(mut Stage s, u8[] next) { return exchange(s.buffer, next); }",
        &["Swap"],
    )[0];
    assert_eq!(assert_guarded_stores(swap), 1, "{swap}");
    assert_eq!(calls_to(swap, ARRAY_ALLOCATE) + calls_to(swap, OBJECT_ALLOCATE), 0, "exchange allocates nothing: {swap}");
    assert_yields_loaded_value(swap);
}

#[test]
fn exchange_of_an_element_stores_the_value_behind_the_barrier_and_yields_the_old_value() {
    let swap = &lower_module(
        "u8[] Swap(u8[][] xs, i64 i, u8[] s) { return exchange(xs[i], s); }",
        &["Swap"],
    )[0];
    assert_eq!(assert_guarded_stores(swap), 1, "{swap}");
    assert!(swap.contains("heap_oob"), "the element exchange stays bounds-checked: {swap}");
    assert_yields_loaded_value(swap);
}
