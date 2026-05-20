//! CLI entry for [`beskid_ast_reflect_gen`].

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use beskid_ast_reflect_gen::{
    default_allowlist_paths, parse_cli_args, run_cli, syntax_nodes::emit_syntax_sdk,
    syntax_nodes::inventory_syntax_type_names, syntax_traversal::emit_query_facade_body,
};

fn run_dump_syntax_inventory(raw: &[OsString]) -> std::process::ExitCode {
    let (workspace, _) = extract_workspace_flag(raw.to_vec());
    let Some(ws) = workspace else {
        eprintln!("error: --dump-syntax-inventory requires --workspace <COMPILER_ROOT>");
        return std::process::ExitCode::from(2);
    };
    let analysis_src = ws.join("crates/beskid_analysis/src");
    match inventory_syntax_type_names(&analysis_src) {
        Ok(names) => {
            for name in names {
                println!("{name}");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Parse argv, optionally fill allowlisted paths from `--workspace`, then generate or emit SDK.
fn main() -> std::process::ExitCode {
    let raw: Vec<OsString> = env::args_os().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return std::process::ExitCode::SUCCESS;
    }

    if raw.iter().any(|a| a == "--dump-syntax-inventory") {
        return run_dump_syntax_inventory(&raw);
    }

    if let Some(idx) = raw.iter().position(|a| a == "--emit-syntax-sdk") {
        return run_emit_syntax_sdk(&raw, idx);
    }

    if raw.iter().any(|a| a == "--emit-query-facade") {
        return run_emit_query_facade(&raw);
    }

    let (workspace, rest) = extract_workspace_flag(raw);

    let mut inv = match parse_cli_args(&rest) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    if inv.paths.is_empty() {
        let Some(ws) = workspace else {
            eprintln!(
                "error: pass one or more .rs files, or use --workspace <compiler-root> for the built-in allowlist."
            );
            print_help();
            return std::process::ExitCode::from(2);
        };
        inv.paths = default_allowlist_paths(ws.as_path());
        let missing: Vec<_> = inv.paths.iter().filter(|p| !p.is_file()).collect();
        if !missing.is_empty() {
            eprintln!("error: missing sources under workspace:");
            for p in missing {
                eprintln!("  {}", p.display());
            }
            return std::process::ExitCode::from(1);
        }
    }

    if let Err(e) = run_cli(inv) {
        eprintln!("error: {e}");
        return std::process::ExitCode::from(1);
    }
    std::process::ExitCode::SUCCESS
}

/// `beskid_ast_reflect_gen --emit-syntax-sdk <DIR> --workspace <ROOT>`: write per-node `.bd` files.
fn run_emit_syntax_sdk(raw: &[OsString], flag_idx: usize) -> std::process::ExitCode {
    let sdk_arg = raw.get(flag_idx + 1).filter(|a| {
        let s = a.to_string_lossy();
        !s.is_empty() && !s.starts_with('-')
    });
    let Some(sdk_arg) = sdk_arg else {
        eprintln!("error: --emit-syntax-sdk requires a directory path (…/compiler-sdk/src/Beskid)");
        return std::process::ExitCode::from(2);
    };
    let sdk_path = PathBuf::from(sdk_arg);
    let mut workspace_root: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == "--workspace" {
            if i + 1 < raw.len() {
                workspace_root = Some(PathBuf::from(&raw[i + 1]));
            }
            break;
        }
        i += 1;
    }
    let Some(ws) = workspace_root else {
        eprintln!(
            "error: --emit-syntax-sdk requires --workspace <COMPILER_ROOT> (directory containing crates/beskid_analysis)"
        );
        return std::process::ExitCode::from(2);
    };
    let analysis_src = ws.join("crates/beskid_analysis/src");
    if !analysis_src.is_dir() {
        eprintln!(
            "error: analysis sources not found at {}",
            analysis_src.display()
        );
        return std::process::ExitCode::from(1);
    }
    match emit_syntax_sdk(&sdk_path, &analysis_src) {
        Ok(rep) => {
            eprintln!(
                "wrote {} syntax node .bd files under {}/Syntax/Nodes/",
                rep.type_names.len(),
                sdk_path.display()
            );
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// `beskid_ast_reflect_gen --emit-query-facade` — print `Beskid.Compiler.Query` facade body (stdout).
fn run_emit_query_facade(raw: &[OsString]) -> std::process::ExitCode {
    let mut workspace_root: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == "--workspace" {
            if i + 1 < raw.len() {
                workspace_root = Some(PathBuf::from(&raw[i + 1]));
            }
            break;
        }
        i += 1;
    }
    let Some(ws) = workspace_root else {
        eprintln!("error: --emit-query-facade requires --workspace <COMPILER_ROOT>");
        return std::process::ExitCode::from(2);
    };
    let analysis_src = ws.join("crates/beskid_analysis/src");
    match inventory_syntax_type_names(&analysis_src) {
        Ok(inv) => {
            print!("{}", emit_query_facade_body(&inv));
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Strip `--workspace <DIR>` from args for `parse_cli_args` while remembering the workspace root.
fn extract_workspace_flag(args: Vec<OsString>) -> (Option<PathBuf>, Vec<OsString>) {
    let mut workspace = None;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--workspace" {
            if i + 1 < args.len() {
                workspace = Some(PathBuf::from(&args[i + 1]));
                i += 2;
                continue;
            }
            eprintln!("error: --workspace requires a directory path");
            std::process::exit(2);
        }
        out.push(args[i].clone());
        i += 1;
    }
    (workspace, out)
}

/// Usage and flags (stderr).
fn print_help() {
    eprintln!(
        "\
beskid_ast_reflect_gen — emit Beskid (.bd) stubs from Rust syntax for reflection bootstrapping.

USAGE:
    cargo run -p beskid_ast_reflect_gen -- [OPTIONS] [--] <FILE.rs>...
    cargo run -p beskid_ast_reflect_gen -- --workspace <COMPILER_ROOT> [OPTIONS]

OPTIONS:
    --emit-syntax-sdk <DIR>  Write per-node .bd files under <DIR>/Syntax/Nodes/ (requires --workspace)
    --workspace <DIR>   Use built-in allowlist under DIR (compiler repo root)
    --out <PATH>        Write output instead of stdout (overrides OUT_DIR)
    --only-annotated    Only emit items marked with #[beskid_reflect]
    --no-banner         Omit the standard generated-file banner
    --no-reflect-stub   Omit `pub type ReflectStub {{}}` (for stitching)
    --items A,B         Only emit listed public enum/type names
    -h, --help          Print this help

ENVIRONMENT:
    OUT_DIR             When set and --out omitted, writes $(OUT_DIR)/ast_reflect/generated.bd

Default allowlist (relative to workspace root):
{allowlist}
",
        allowlist = beskid_ast_reflect_gen::DEFAULT_ANALYSIS_ALLOWLIST
            .iter()
            .map(|s| format!("    {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
