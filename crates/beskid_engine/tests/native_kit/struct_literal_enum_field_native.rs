//! A generic enum constructor written as a struct literal field value, end to end on the
//! canonical runtime kit: the constructor takes its application from the declared field type.

use std::path::Path;

use crate::explicit_kit::run_entrypoint_with_kit;
use crate::shared_kit;

const SOURCE: &str = r#"
enum Option<T> { Some(T value), None() }
type Holder<T> { Option<i64[]> window; Option<T> item; i64 count; }
Holder<i64> Make() {
    return Holder<i64> { window: Option::Some([30_i64, 8_i64]), item: Option::Some(3_i64), count: 1_i64 };
}
i64 Main() {
    Holder<i64> holder = Make();
    return match holder.window { Option::Some(values) => 41_i64 + holder.count, Option::None() => -100_i64, };
}
"#;

#[test]
fn generic_enum_constructors_in_struct_literal_fields_run() {
    let kit = shared_kit::debug();
    let output = run_entrypoint_with_kit(
        kit.prefix(),
        Path::new("struct-literal-enum-field.bd"),
        SOURCE,
        "Main",
        &beskid_engine::ModHost::NoMods,
    )
    .expect("generic enum constructors in struct literal fields lower and run");
    assert_eq!(output, "42", "the constructed field payload and the count survive construction");
}
