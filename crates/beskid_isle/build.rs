use std::path::{Path, PathBuf};

const INPUTS: &[&str] = &[
    "types.isle",
    "ast.isle",
    "expressions.isle",
    "literals.isle",
    "binary.isle",
    "unary_casts.isle",
    "calls.isle",
    "statements.isle",
    "control_flow.isle",
    "memory.isle",
    "runtime_intrinsics.isle",
    "items.isle",
];

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must set CARGO_MANIFEST_DIR"),
    );
    let isle_dir = manifest_dir.join("isle");
    let inputs = INPUTS
        .iter()
        .map(|name| isle_dir.join(name))
        .collect::<Vec<_>>();

    for input in &inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }

    let options = cranelift_isle::codegen::CodegenOptions {
        exclude_global_allow_pragmas: true,
        prefixes: vec![cranelift_isle::codegen::Prefix {
            prefix: path_text(&manifest_dir),
            name: "$BESKID_ISLE".to_owned(),
        }],
    };
    let generated = cranelift_isle::compile::from_files(&inputs, &options)
        .unwrap_or_else(|errors| panic!("failed to compile Beskid ISLE rules:\n{errors:?}"));

    let rule_count = inputs
        .iter()
        .map(|input| std::fs::read_to_string(input).expect("read ISLE input"))
        .map(|source| source.matches("(rule").count())
        .sum::<usize>();
    assert!(rule_count > 0, "Beskid ISLE must contain at least one rule");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    std::fs::write(out_dir.join("beskid_lower.rs"), generated).expect("write generated ISLE Rust");
    std::fs::write(
        out_dir.join("beskid_isle_metadata.rs"),
        format!("pub const RULE_COUNT: usize = {rule_count};\n"),
    )
    .expect("write generated ISLE metadata");
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
