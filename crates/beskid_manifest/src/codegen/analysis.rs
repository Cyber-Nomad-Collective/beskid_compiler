use std::fmt::Write as _;

use crate::model::ManifestRoot;

use super::common::{dispatch_groups, language_handler_beskid_path, write_generated_preamble};

pub fn render_analysis_builtins(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "define_builtins! {{").unwrap();

    for intrinsic in &manifest.intrinsic {
        write_analysis_entry(
            &mut out,
            &intrinsic.path,
            &intrinsic.symbol,
            &intrinsic.params,
            &intrinsic.returns,
            intrinsic.injected,
        );
    }

    for entry in &manifest.kernel {
        if entry.beskid_path.is_empty() {
            continue;
        }
        write_analysis_entry(
            &mut out,
            &entry.beskid_path,
            &entry.symbol,
            &entry.params,
            &entry.returns,
            entry.injected,
        );
    }

    for group in dispatch_groups(manifest) {
        for entry in group {
            if entry.beskid_path.is_empty() {
                continue;
            }
            write_analysis_entry(
                &mut out,
                &entry.beskid_path,
                &entry.dispatch_key,
                &entry.params,
                &entry.returns,
                entry.injected,
            );
            if entry.is_language_handler()
                && let Some(handler_path) = language_handler_beskid_path(entry)
            {
                write_analysis_entry(
                    &mut out,
                    &handler_path,
                    &entry.dispatch_key,
                    &entry.params,
                    &entry.returns,
                    entry.injected,
                );
            }
        }
    }

    writeln!(&mut out, "}}").unwrap();
    out
}

/// Render the analysis builtin surface plus manifest-owned ABI-v5 runtime intrinsic candidates
/// and process-linked soft builtins.
///
/// These entries make exact canonical-runtime calls resolvable by the syntax fact layer. They do
/// not grant an ABI import: `CodegenInput::runtime_intrinsic_for` still requires the opaque
/// canonical-runtime capability before codegen can emit the symbol.
pub fn append_analysis_v5_intrinsics(base: &str, runtime: &crate::v5::RuntimeManifestV5) -> String {
    const MARKER: &str = "// ABI-v5 canonical runtime declarations\n";
    const LEGACY_MARKER: &str = "// ABI-v5 canonical runtime intrinsic candidates\n";
    let generated_start =
        [base.find(MARKER), base.find(LEGACY_MARKER)].into_iter().flatten().min().unwrap_or(base.len());
    let mut out = base[..generated_start].to_owned();
    if !out.trim_end().ends_with('}') {
        out.push_str("}\n");
    }
    let closing = out.rfind('}').expect("generated builtins closes macro");
    let mut intrinsic_entries = String::from(MARKER);
    for intrinsic in &runtime.intrinsics {
        let params = intrinsic.params.iter().map(|parameter| v5_analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_analysis_entry(
            &mut intrinsic_entries,
            std::slice::from_ref(&intrinsic.name),
            &intrinsic.symbol,
            &params,
            &v5_analysis_type(&intrinsic.result),
            true,
        );
    }
    for builtin in &runtime.soft_builtins {
        let params = builtin.params.iter().map(|parameter| v5_analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_analysis_entry(
            &mut intrinsic_entries,
            std::slice::from_ref(&builtin.name),
            &builtin.symbol,
            &params,
            &v5_analysis_type(&builtin.result),
            true,
        );
    }
    out.insert_str(closing, &intrinsic_entries);
    out
}

fn v5_analysis_type(ty: &str) -> String {
    match ty {
        "pointer" => "ptr".into(),
        "string" => "string".into(),
        // The legacy resolver surface only distinguishes wide numeric scalar candidates. Exact
        // ABI widths come from the canonical manifest in syntax codegen, not this lookup table.
        "u8" | "u32" | "i32" | "i64" | "isize" => "u64".into(),
        "void" => "unit".into(),
        other => other.into(),
    }
}

pub fn render_runtime_handler_specs(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "/// Language-owned handler metadata merged from manifest `language_handler` rows.").unwrap();
    writeln!(&mut out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(&mut out, "pub struct RuntimeHandlerSpec {{").unwrap();
    writeln!(&mut out, "    pub dispatch_key: &'static str,").unwrap();
    writeln!(&mut out, "    pub tag: u32,").unwrap();
    writeln!(&mut out, "    pub return_group: &'static str,").unwrap();
    writeln!(&mut out, "    pub handler_path: &'static [&'static str],").unwrap();
    writeln!(&mut out, "}}").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "pub const RUNTIME_HANDLER_SPECS: &[RuntimeHandlerSpec] = &[").unwrap();
    for (group, entries) in [
        ("usize", &manifest.dispatch.usize),
        ("ptr", &manifest.dispatch.ptr),
        ("unit", &manifest.dispatch.unit),
        ("i64", &manifest.dispatch.i64),
    ] {
        for entry in entries {
            if let Some(handler_path) = language_handler_beskid_path(entry) {
                let path =
                    handler_path.iter().map(|segment| format!("\\\"{segment}\\\"")).collect::<Vec<_>>().join(", ");
                writeln!(
                    &mut out,
                    "    RuntimeHandlerSpec {{ dispatch_key: \"{}\", tag: {}, return_group: \"{group}\", handler_path: &[{path}] }},",
                    entry.dispatch_key, entry.tag,
                )
                .unwrap();
            }
        }
    }
    writeln!(&mut out, "]; ").unwrap();
    out
}

fn write_analysis_entry(
    out: &mut String,
    path: &[String],
    symbol: &str,
    params: &[String],
    returns: &str,
    injected: bool,
) {
    let path_lit = path.iter().map(|segment| format!("\"{segment}\"")).collect::<Vec<_>>().join(", ");
    let params_lit = params.iter().map(|param| analysis_type_ident(param)).collect::<Vec<_>>().join(", ");
    let returns_lit = analysis_type_ident(returns);
    writeln!(out, "    &[{path_lit}] => {{").unwrap();
    writeln!(out, "        symbol: \"{symbol}\",").unwrap();
    writeln!(out, "        params: [{params_lit}],").unwrap();
    writeln!(out, "        returns: {returns_lit},").unwrap();
    writeln!(out, "        injected: {injected},").unwrap();
    writeln!(out, "    }},").unwrap();
}

fn analysis_type_ident(param: &str) -> String {
    match param {
        "string" => "String".to_string(),
        "ptr" => "Ptr".to_string(),
        "usize" => "Usize".to_string(),
        "u64" | "i64" | "i32" => "U64".to_string(),
        "f64" => "F64".to_string(),
        "unit" | "void" => "Unit".to_string(),
        "never" => "Never".to_string(),
        other => other.to_string(),
    }
}
