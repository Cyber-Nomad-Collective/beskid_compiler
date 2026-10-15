//! A unary operator over a nested field read as the value of a field write
//! (`this.state.active = !this.state.input.failed`), end to end on the canonical runtime kit.

use std::path::Path;

use beskid_engine::services::run_entrypoint;

use crate::runtime_prefix::RuntimePrefixContext;
use crate::shared_kit;

const SOURCE: &str = r#"
type Input { bool failed, i64 nodes }
type State { Input input, bool active, i64 count }
type Reader {
    State state,
    pub i64 Go() {
        this.state.active = !this.state.input.failed;
        this.state.count = -this.state.input.nodes;
        if (this.state.active) { return 0_i64 - this.state.count; }
        return 0_i64;
    }
}
i64 Main() {
    Reader reader = Reader { state: State { input: Input { failed: false, nodes: 42_i64 }, active: false, count: 0_i64 } };
    return reader.Go();
}
"#;

#[test]
fn unary_operators_over_nested_field_reads_run_as_field_write_values() {
    let kit = shared_kit::debug();
    let _runtime_prefix = RuntimePrefixContext::install(kit.prefix());
    let output = run_entrypoint(Path::new("unary-field-write.bd"), SOURCE, "Main", &beskid_engine::ModHost::NoMods)
        .expect("unary field write values lower and run");
    assert_eq!(output, "42", "the negated flag selects the branch and the negated count is restored");
}
