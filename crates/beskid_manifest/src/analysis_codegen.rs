use std::fmt::Write as _;

use crate::v5::RuntimeManifestV5;

pub fn append_analysis_intrinsics(base: &str, runtime: &RuntimeManifestV5) -> String {
    const MARKER: &str = "// ABI-v5 canonical runtime declarations\n";
    const OLD_MARKER: &str = "// ABI-v5 canonical runtime intrinsic candidates\n";
    let generated_start = [base.find(MARKER), base.find(OLD_MARKER)].into_iter().flatten().min().unwrap_or(base.len());
    let mut out = base[..generated_start].to_owned();
    if !out.trim_end().ends_with('}') {
        out.push_str("}\n");
    }
    let closing = out.rfind('}').expect("generated builtins closes macro");
    let mut entries = String::from(MARKER);
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
        "u8" | "u32" | "i32" | "i64" | "isize" => "u64".into(),
        "void" => "unit".into(),
        other => other.into(),
    }
}

fn type_ident(ty: &str) -> &'static str {
    match ty {
        "string" => "String",
        "ptr" => "Ptr",
        "usize" => "Usize",
        "u64" | "i64" | "i32" => "U64",
        "f64" => "F64",
        "unit" | "void" => "Unit",
        "never" => "Never",
        _ => "Unit",
    }
}
