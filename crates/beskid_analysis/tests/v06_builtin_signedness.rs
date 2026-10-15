//! Generated compiler declarations must retain manifest signedness.
use beskid_analysis::builtins::builtin_for_path;

#[test]
fn canonical_clock_and_status_builtins_do_not_become_unsigned() {
    for (name, expected) in [("clock_monotonic_nanos", "I64"), ("network_submit", "I32"), ("raw_word_load", "Usize")] {
        let (_, builtin) = builtin_for_path(&[name.to_owned()]).expect("canonical runtime builtin");
        assert_eq!(format!("{:?}", builtin.returns), expected, "{name} must retain its declared representation");
    }
}
