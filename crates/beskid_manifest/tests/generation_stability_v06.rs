use beskid_manifest::generate_analysis_with_v5_intrinsics_from_source;
use std::fs::{self, File, FileTimes};
use std::time::{Duration, SystemTime};

#[test]
fn v06_unchanged_builtin_generation_preserves_mtime_and_bytes() {
    let directory = std::env::temp_dir().join(format!(
        "beskid-builtin-generation-{}-{}", std::process::id(),
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let output = directory.join("builtins.inc.rs");
    let source = include_str!("../../../runtime_manifest.bsol");
    let base = include_str!("../../beskid_analysis/src/generated/builtins.inc.rs");
    generate_analysis_with_v5_intrinsics_from_source(source, base, &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    let sentinel = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    File::options().write(true).open(&output).unwrap()
        .set_times(FileTimes::new().set_modified(sentinel)).unwrap();
    let before = fs::metadata(&output).unwrap().modified().unwrap();
    let generated = fs::read_to_string(&output).unwrap();
    generate_analysis_with_v5_intrinsics_from_source(source, &generated, &output).unwrap();
    let after = fs::metadata(&output).unwrap().modified().unwrap();
    assert_eq!(fs::read(&output).unwrap(), bytes);
    assert_eq!(after, before, "unchanged generation must not invalidate its own Cargo build input");
    let changed_base = format!("// updated baseline\n{generated}");
    generate_analysis_with_v5_intrinsics_from_source(source, &changed_base, &output).unwrap();
    let changed = fs::read(&output).unwrap();
    assert_ne!(changed, bytes, "real baseline changes must still regenerate output");
    assert!(changed.starts_with(b"// updated baseline\n"));
    assert!(generate_analysis_with_v5_intrinsics_from_source("invalid ABI", &changed_base, &output).is_err());
    assert_eq!(fs::read(&output).unwrap(), changed, "invalid ABI cannot replace the existing output");
    fs::remove_dir_all(directory).unwrap();
}
