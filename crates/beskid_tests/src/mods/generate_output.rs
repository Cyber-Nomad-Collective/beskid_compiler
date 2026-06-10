//! Generic mod.generate output materialization tests.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::mod_host::{
    CodeGenerateOutput, GenerateOutputFile, GenerateOutputLayout, materialize_program_items,
    write_code_generate_output, write_typed_generate_output,
};

#[test]
fn write_code_generate_output_honors_layout_manifest() {
    let package = unique_temp_dir("mod_code_generate_output_layout");
    fs::create_dir_all(&package).expect("mkdir");
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
    write_code_generate_output(
        None,
        &package,
        &layout,
        &[CodeGenerateOutput {
            module_path: String::new(),
            body: "pub i64 Demo() { return 1; }".into(),
        }],
    )
    .expect("write");
    let written =
        fs::read_to_string(package.join(".generated/Core/Text/Regex/Generated.g.bd")).expect("read");
    assert!(written.starts_with("// layout test\n"));
    assert!(written.contains("pub i64 Demo()"));
    let _ = fs::remove_dir_all(package);
}

#[test]
fn write_typed_generate_output_honors_layout_manifest() {
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
    write_typed_generate_output(&output, &items, Some(&layout)).expect("write");
    let written = fs::read_to_string(output.join("Demo.bd")).expect("read");
    assert!(written.starts_with("// layout test\n"));
    assert!(written.contains("pub contract DemoStep"));
    let _ = fs::remove_dir_all(output);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{id}"))
}
