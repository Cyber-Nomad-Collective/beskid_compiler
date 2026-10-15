use beskid_analysis::services::parse_program_with_source_name_and_diagnostics;
use std::path::Path;

#[test]
fn v06_process_public_and_runtime_sources_are_strict_syntax() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in [
        "corelib/packages/foundation/src/Core/Process/Process.bd",
        "corelib/packages/foundation/src/Core/Time/Deadline.bd",
        "runtime/beskid/src/Runtime/Host/Process.bd",
    ] {
        let path = root.join(relative);
        let source = std::fs::read_to_string(&path).unwrap();
        let parsed = parse_program_with_source_name_and_diagnostics(path.to_str().unwrap(), &source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            !parsed.recovered && parsed.diagnostics.is_empty(),
            "{} was repaired or diagnosed: {:?}",
            path.display(),
            parsed.diagnostics
        );
    }
}
