//! `beskid doc --project` on a package with Mod-generated units (a `.generated` sibling of the source
//! root, as corelib_pest_gen writes for corelib_foundation) must emit artifact-relative `api.json`
//! paths: `.generated/<rel>` for generated units and `src/<rel>` for regular units. Regression for the
//! 0.6.0 Corelib publication, where the materialized `obj/beskid/root/.generated` unit failed to
//! relativize against `obj/beskid/root/src`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::doc::{ApiDocRoot, path_looks_absolute};

fn corelib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib").canonicalize().expect("compiler corelib checkout")
}

#[test]
fn doc_project_maps_generated_units_to_the_generated_artifact_prefix() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid_doc_generated_{}_{nonce}", std::process::id()));
    fs::create_dir_all(root.join("src/Gen")).expect("source directory");
    fs::create_dir_all(root.join(".generated/Gen/Grammar")).expect("generated directory");
    fs::write(
        root.join("gen_pkg.bproj"),
        "gen_pkg { name = \"gen_pkg\" version = \"0.1.0\" root = \"src\" }\n\
         target \"GenLib\" { kind = Lib }\n",
    )
    .expect("manifest");
    fs::write(root.join("src/Gen/Api.bd"), "/// Regular unit.\npub i64 GenFixtureAnswer() {\n    return 42;\n}\n")
        .expect("regular unit");
    fs::write(
        root.join(".generated/Gen/Grammar/Generated.g.bd"),
        "/// Generated unit.\npub i64 GenFixtureDigit() {\n    return 7;\n}\n",
    )
    .expect("generated unit");

    let out = root.join("docout");
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["doc", "--project"])
        .arg(root.join("gen_pkg.bproj"))
        .arg("--out")
        .arg(&out)
        .env("BESKID_CORELIB_ROOT", corelib_root())
        .current_dir(&root)
        .output()
        .expect("run beskid doc");
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "doc --project with a generated unit must succeed: {text}");

    let api: ApiDocRoot = serde_json::from_str(&fs::read_to_string(out.join("api.json")).expect("read api.json"))
        .expect("parse api.json");
    assert!(!path_looks_absolute(&api.source), "source must be artifact-relative: {}", api.source);
    for item in &api.items {
        assert!(!path_looks_absolute(&item.location.file), "{}: {}", item.qualified_name, item.location.file);
        assert!(!item.location.file.contains("obj/beskid"), "{}: {}", item.qualified_name, item.location.file);
    }
    let file_of = |name: &str| {
        api.items
            .iter()
            .find(|item| item.name == name && item.kind == "function" && item.declaring_package.is_none())
            .unwrap_or_else(|| panic!("api.json lists {name}: {api:?}"))
            .location
            .file
            .clone()
    };
    let files: Vec<_> = api
        .items
        .iter()
        .filter(|item| item.declaring_package.is_none())
        .map(|item| format!("{}={}", item.name, item.location.file))
        .collect();
    assert_eq!(file_of("GenFixtureDigit"), ".generated/Gen/Grammar/Generated.g.bd", "items: {files:?}");
    // Regular units are folded into the materialized workspace entry (`src/__workspace_entry__.bd`,
    // as for the corelib packages), so only the artifact prefix of regular items is stable.
    let answer = file_of("GenFixtureAnswer");
    assert!(answer.starts_with("src/"), "regular unit must map under src/: {answer}; items: {files:?}");

    let _ = fs::remove_dir_all(&root);
}

/// The Corelib publication documents these packages directly (scripts/ci/lib/corelib-publish-runner.mjs:
/// `BESKID_CORELIB_ROOT` is the Corelib root). corelib_foundation compiles its own generated regex unit
/// from `obj/beskid/root/.generated`; corelib_runtime and corelib_concurrency list corelib_foundation
/// items from `obj/beskid/deps/src/corelib_foundation-<hash>/`. Every path must be artifact-relative to
/// the item's own package (`declaringPackage`, or the published package when absent).
#[test]
fn corelib_publication_packages_emit_artifact_relative_api_paths() {
    let corelib = corelib_root();
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let out_root = std::env::temp_dir().join(format!("beskid_doc_corelib_{}_{nonce}", std::process::id()));
    for (package, manifest) in [
        ("foundation", "corelib_foundation.bproj"),
        ("runtime", "corelib_runtime.bproj"),
        ("concurrency", "corelib_concurrency.bproj"),
    ] {
        let out = out_root.join(package);
        let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
            .args(["doc", "--project"])
            .arg(corelib.join("packages").join(package).join(manifest))
            .arg("--out")
            .arg(&out)
            .env("BESKID_CORELIB_ROOT", &corelib)
            .current_dir(&corelib)
            .output()
            .expect("run beskid doc");
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "{package}: beskid doc must succeed: {text}");

        let api: ApiDocRoot = serde_json::from_str(&fs::read_to_string(out.join("api.json")).expect("read api.json"))
            .expect("parse api.json");
        assert!(api.source.starts_with("src/"), "{package}: source {}", api.source);
        for item in &api.items {
            let file = &item.location.file;
            assert!(
                file.starts_with("src/") || file.starts_with(".generated/"),
                "{package}: {} ({:?}) has non-artifact path {file}",
                item.qualified_name,
                item.declaring_package
            );
        }
        let generated = |declaring: Option<&str>| {
            api.items.iter().any(|item| {
                item.location.file.starts_with(".generated/") && item.declaring_package.as_deref() == declaring
            })
        };
        match package {
            "foundation" => assert!(generated(None), "foundation lists its own generated regex unit"),
            _ => assert!(
                api.items.iter().any(|item| item.declaring_package.as_deref() == Some("corelib_foundation")),
                "{package} lists corelib_foundation items with their declaring package"
            ),
        }
    }
    let _ = fs::remove_dir_all(&out_root);
}
