#![cfg(any(unix, windows))]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::projects::assembly::UnitBuilder;
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_artifacts::{ArtifactStore, content_fingerprint};

const SOURCE: &str = "i32 Main() { return 0; }";

struct ProjectRoot(PathBuf);

impl ProjectRoot {
    fn new() -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid_origin_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("create temporary project");
        Self(path)
    }
}

impl Drop for ProjectRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn symlink_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("create source symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).expect("create source symlink");
}

#[test]
fn builder_retains_symlink_request_origin_on_cold_parse() {
    let root = ProjectRoot::new();
    let real = root.0.join("Real.bd");
    let alias = root.0.join("Alias.bd");
    fs::write(&real, SOURCE).unwrap();
    symlink_file(&real, &alias);

    let (unit, _) = UnitBuilder::new(&root.0).build_unit(&alias, SOURCE, SyntaxGenerationId(1)).unwrap();

    assert_eq!(unit.origin_path, alias);
    assert_eq!(unit.path, fs::canonicalize(real).unwrap());
    assert_eq!(unit.logical_name, alias.display().to_string());
}

#[test]
fn builder_rebinds_disk_snapshot_origin_in_both_request_orders() {
    for use_copy in [false, true] {
        for alias_first in [false, true] {
            let root = ProjectRoot::new();
            let real = root.0.join("Real.bd");
            let alias = root.0.join("Alias.bd");
            fs::write(&real, SOURCE).unwrap();
            if use_copy {
                fs::copy(&real, &alias).unwrap();
            } else {
                symlink_file(&real, &alias);
            }
            let (first, second) = if alias_first { (&alias, &real) } else { (&real, &alias) };
            let builder = UnitBuilder::new(&root.0);
            let (first_unit, _) = builder.build_unit(first, SOURCE, SyntaxGenerationId(1)).unwrap();
            assert_eq!(&first_unit.origin_path, first);
            assert!(ArtifactStore::new(&root.0).read_ast(&content_fingerprint(SOURCE)).is_some());

            let unexpected_parse = |_: &Path, _: &str, _: SyntaxGenerationId| panic!("expected a disk snapshot hit");
            let cached_builder = UnitBuilder::new(&root.0).with_salsa_build(&unexpected_parse);
            let (second_unit, _) = cached_builder.build_unit(second, SOURCE, SyntaxGenerationId(2)).unwrap();
            assert_eq!(&second_unit.origin_path, second);
            assert_eq!(second_unit.path, fs::canonicalize(second).unwrap());
            assert_eq!(second_unit.logical_name, second.display().to_string());
        }
    }
}
