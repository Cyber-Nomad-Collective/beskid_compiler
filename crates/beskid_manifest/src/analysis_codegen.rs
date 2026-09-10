use std::fmt::Write as _;

use crate::v5::RuntimeManifestV5;

pub fn append_analysis_intrinsics(base: &str, runtime: &RuntimeManifestV5) -> String {
    const MARKER: &str = "// ABI-v5 canonical runtime declarations";
    const OLD_MARKER: &str = "// ABI-v5 canonical runtime intrinsic candidates";
    let generated_start = [base.find(MARKER), base.find(OLD_MARKER)].into_iter().flatten().min().unwrap_or(base.len());
    let mut out = base[..generated_start].to_owned();
    if !out.trim_end().ends_with('}') {
        out.push_str("}\n");
    }
    let closing = out.rfind('}').expect("generated builtins closes macro");
    let mut entries = format!("{MARKER}\n");
    for intrinsic in &runtime.intrinsics {
        let params = intrinsic.params.iter().map(|parameter| analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_entry(&mut entries, &intrinsic.name, &intrinsic.symbol, &params, &analysis_type(&intrinsic.result));
    }
    for builtin in &runtime.soft_builtins {
        let params = builtin.params.iter().map(|parameter| analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_entry(&mut entries, &builtin.name, &builtin.symbol, &params, &analysis_type(&builtin.result));
    }
    out.insert_str(closing, &entries);
    out
}

fn write_entry(out: &mut String, name: &str, symbol: &str, params: &[String], result: &str) {
    let params = params.iter().map(|param| type_ident(param)).collect::<Vec<_>>().join(", ");
    writeln!(out, "    &[\"{name}\"] => {{").unwrap();
    writeln!(out, "        symbol: \"{symbol}\",").unwrap();
    writeln!(out, "        params: [{params}],").unwrap();
    writeln!(out, "        returns: {},", type_ident(result)).unwrap();
    writeln!(out, "        injected: true,").unwrap();
    writeln!(out, "    }},").unwrap();
}

fn analysis_type(ty: &str) -> String {
    match ty {
        "pointer" => "ptr".into(),
        "string" => "string".into(),
        "u32" => "u32".into(),
        "u8" | "i32" | "i64" | "isize" => "u64".into(),
        "void" => "unit".into(),
        other => other.into(),
    }
}

fn type_ident(ty: &str) -> &'static str {
    match ty {
        "string" => "String",
        "ptr" => "Ptr",
        "usize" => "Usize",
        "u32" => "U32",
        "u64" | "i64" | "i32" => "U64",
        "f64" => "F64",
        "unit" | "void" => "Unit",
        "never" => "Never",
        _ => "Unit",
    }
}

#[cfg(test)]
mod tests {
    use super::append_analysis_intrinsics;

    #[test]
    fn generated_builtin_block_is_idempotent_with_crlf_input() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime_manifest.bsol"));
        let runtime = crate::load_v5_manifest_source(source).expect("canonical runtime manifest");
        let checked_in =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../beskid_analysis/src/generated/builtins.inc.rs"));
        let crlf = checked_in.replace('\n', "\r\n");

        let once = append_analysis_intrinsics(&crlf, &runtime);
        let twice = append_analysis_intrinsics(&once, &runtime);

        assert_eq!(once.matches("// ABI-v5 canonical runtime declarations").count(), 1);
        assert_eq!(once.matches("&[\"native_word_from_pointer\"]").count(), 1);
        assert_eq!(twice, once);
    }
}
