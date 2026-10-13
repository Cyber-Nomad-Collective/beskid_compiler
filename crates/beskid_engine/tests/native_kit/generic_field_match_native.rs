//! Matches over a field of a generic record (`Option<T[]> slot` of `Holder<T>`) inside generic
//! specializations, end to end on the canonical runtime kit: wildcard payloads, value matches
//! bound with an unannotated `let`, indexed payload bindings, the `this.slot`, bare `slot`,
//! `h.slot`, `(h).slot` and nested `o.inner.slot` spellings, and a guarded statement match, for a
//! scalar and a managed record `T` and both variants.

use std::path::Path;

use beskid_engine::services::run_entrypoint;

use crate::runtime_prefix::RuntimePrefixContext;
use crate::shared_kit;

const SOURCE: &str = r#"
enum Option<T> { Some(T value), None }
type Point { i64 x }
type Holder<T> {
    Option<T[]> slot,
    i64 Has() { return match this.slot { Option::Some(_) => 1_i64, Option::None => 0_i64, }; }
    T FirstOr(T fallback) { let x = match slot { Option::Some(values) => values[0], Option::None => fallback, }; return x; }
}
type Outer<T> { Holder<T> inner }
i64 HasFn<T>(Holder<T> h) { return match (h).slot { Option::Some(_) => 10_i64, Option::None => 0_i64, }; }
T SecondOr<T>(Holder<T> h, T fallback) { let x = match h.slot { Option::Some(values) => values[1], Option::None => fallback, }; return x; }
i64 Nested<T>(Outer<T> o, i64 limit) {
    mut i64 total = 0_i64;
    match o.inner.slot {
        Option::Some(_) when limit > 0_i64 => { total = 100_i64; },
        Option::Some(_) => { total = 200_i64; },
        Option::None => { total = 300_i64; },
    };
    return total;
}
i64 Main() {
    i64[] numbers = [3_i64, 4_i64];
    Holder<i64> ints = Holder<i64> { slot: Option::Some(numbers) };
    Holder<i64> empty = Holder<i64> { slot: Option::None };
    Point[] points = [Point { x: 5_i64 }, Point { x: 6_i64 }];
    Holder<Point> boxed = Holder<Point> { slot: Option::Some(points) };
    Point first = boxed.FirstOr(Point { x: -1_i64 });
    Point second = SecondOr<Point>(boxed, Point { x: -1_i64 });
    i64 has = ints.Has() + empty.Has() + HasFn<i64>(ints) + HasFn<Point>(boxed);
    i64 values = ints.FirstOr(-1_i64) + empty.FirstOr(7_i64) + SecondOr<i64>(ints, -1_i64) + first.x + second.x;
    i64 nested = Nested<i64>(Outer<i64> { inner: ints }, 1_i64) + Nested<Point>(Outer<Point> { inner: boxed }, 0_i64)
        + Nested<i64>(Outer<i64> { inner: empty }, 1_i64);
    return has + values + nested;
}
"#;

#[test]
fn matches_on_generic_record_fields_run_inside_their_specializations() {
    let kit = shared_kit::debug();
    let _runtime_prefix = RuntimePrefixContext::install(kit.prefix());
    let output = run_entrypoint(Path::new("generic-field-match.bd"), SOURCE, "Main", &beskid_engine::ModHost::NoMods)
        .expect("matches on generic record fields lower and run in their specializations");
    // has 1 + 0 + 10 + 10, values 3 + 7 + 4 + 5 + 6, nested 100 + 200 + 300.
    assert_eq!(output, "646");
}
