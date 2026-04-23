use std::fs;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{Object, ObjectSymbol};

use super::*;

/// Collect symbol names from every object file embedded in a static `ar` archive.
///
/// System `nm` from Xcode cannot parse LLVM bitcode / metadata produced by newer Rust
/// toolchains, so we use the `object` crate (same stack as codegen) for inspection.
fn static_archive_symbol_text(path: &Path) -> String {
    let data = fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let archive = ArchiveFile::parse(data.as_slice())
        .unwrap_or_else(|err| panic!("parse archive {}: {err:?}", path.display()));
    let mut text = String::new();
    for member in archive.members() {
        let Ok(member) = member else {
            continue;
        };
        let Ok(member_data) = member.data(data.as_slice()) else {
            continue;
        };
        if member_data.is_empty() {
            continue;
        }
        let Ok(obj) = object::read::File::parse(member_data) else {
            continue;
        };
        for symbol in obj.symbols() {
            if let Ok(name) = symbol.name() {
                text.push_str(name);
                text.push('\n');
            }
        }
    }
    text
}

#[test]
fn static_build_contains_required_runtime_symbols() {
    let artifact = lower_sample_artifact();
    let dir = temp_case_dir("static_with_runtime_symbols");
    let output = dir.join("libsample.a");

    let result = build(AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::StaticLib,
        output_path: output,
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: "main".to_owned(),
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime: RuntimeStrategy::BuildOnTheFly,
        verbose_link: false,
    })
    .expect("aot static build");

    let final_path = result
        .final_path
        .expect("static build should emit final archive");
    assert!(
        final_path.exists(),
        "expected final static archive to exist"
    );

    let symbols = static_archive_symbol_text(&final_path);
    assert!(
        symbols.contains(SYM_ABI_VERSION),
        "expected final static artifact to expose ABI version symbol"
    );
    assert!(
        symbols.contains(SYM_INTEROP_DISPATCH_UNIT),
        "expected final static artifact to expose interop dispatch symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}
