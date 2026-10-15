//! Generic mod.generate output materialization tests: typed items only.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::mod_host::{
    GenerateOutputFile, GenerateOutputLayout, materialize_program_items, write_typed_generate_output,
};

#[test]
fn write_typed_generate_output_v2_renders_items_under_package_generated_root() {
    let package = unique_temp_dir("mod_generate_output_v2");
    let items = materialize_program_items(["pub i64 Demo() { return 1; }"]).expect("materialize");
    let layout = GenerateOutputLayout {
        schema_version: 2,
        files: vec![GenerateOutputFile {
            path: String::new(),
            header: "// layout test\n".into(),
            item_count: 0,
            file_name: "Generated".into(),
            module_path: "Core.Text.Regex.Generated".into(),
            package_id: String::new(),
        }],
    };
    write_typed_generate_output(None, &package, &package.join("Generated"), &items, &layout).expect("write");
    let written = fs::read_to_string(package.join(".generated/Core/Text/Regex/Generated.g.bd")).expect("read");
    assert!(written.starts_with("// layout test\n"));
    assert!(written.contains("pub i64 Demo()"));
    let _ = fs::remove_dir_all(package);
}

#[test]
fn write_typed_generate_output_v1_honors_layout_manifest() {
    let items = materialize_program_items([
        "pub contract DemoStep { DemoStep Run(); }",
        "pub type DemoFluent { i64 inner }",
        "pub DemoFluent FromDemo() { return DemoFluent { inner: 0 }; }",
    ])
    .expect("materialize");
    let layout = GenerateOutputLayout {
        schema_version: 1,
        files: vec![GenerateOutputFile {
            path: "Demo.bd".into(),
            header: "// layout test\n".into(),
            item_count: 3,
            file_name: String::new(),
            module_path: String::new(),
            package_id: String::new(),
        }],
    };
    let output = unique_temp_dir("mod_generate_output_layout");
    write_typed_generate_output(None, &output, &output, &items, &layout).expect("write");
    let written = fs::read_to_string(output.join("Demo.bd")).expect("read");
    assert!(written.starts_with("// layout test\n"));
    assert!(written.contains("pub contract DemoStep"));
    let _ = fs::remove_dir_all(output);
}

/// The checked-in Pest Mod layout declares one v2 file owned by Foundation; it receives every
/// typed item and needs no source-text contribution.
#[test]
fn corelib_pest_gen_layout_is_a_single_typed_v2_file() {
    let layout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/mods/corelib_pest_gen/generate.layout.json");
    let layout = beskid_analysis::mod_host::load_generate_output_layout(&layout).expect("Pest Mod layout");
    assert_eq!(layout.schema_version, 2);
    assert_eq!(layout.files.len(), 1);
    assert_eq!(layout.files[0].item_count, 0, "a single v2 file receives every generated item");
    assert_eq!(layout.files[0].module_path, "Core.Text.Regex.Generated");
    assert_eq!(layout.files[0].package_id, "corelib_foundation");
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{}_{id}", std::process::id()))
}
