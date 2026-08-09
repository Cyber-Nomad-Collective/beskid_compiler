use std::collections::HashMap;
use std::fmt::Write as _;

use crate::model::ManifestRoot;

use super::common::{symbol_const_suffix, write_generated_preamble};

pub fn render_abi_builtins(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "use crate::abi_types::{{AbiParamKind, AbiReturnKind, BuiltinFnSpec}};").unwrap();
    writeln!(&mut out, "use crate::generated::symbols::{{").unwrap();
    for entry in &manifest.kernel {
        writeln!(&mut out, "    SYM_{},", symbol_const_suffix(&entry.symbol)).unwrap();
    }
    writeln!(&mut out, "}};").unwrap();
    writeln!(&mut out).unwrap();

    let mut param_arrays: HashMap<Vec<AbiParamKind>, String> = HashMap::new();
    let mut next_id = 0usize;
    let mut specs = Vec::new();

    for entry in &manifest.kernel {
        let params = manifest_param_kinds(&entry.params);
        let name = param_array_name(&params, &mut param_arrays, &mut next_id);
        let returns = manifest_return_abi(&entry.returns);
        specs.push((symbol_const_suffix(&entry.symbol), name, returns));
    }

    let mut param_declarations = param_arrays.iter().collect::<Vec<_>>();
    param_declarations.sort_unstable_by_key(|(_, name)| *name);
    for (params, name) in param_declarations {
        let formatted = format_param_array(params);
        writeln!(&mut out, "const {name}: [AbiParamKind; {len}] = {formatted};", len = params.len()).unwrap();
    }
    writeln!(&mut out).unwrap();

    writeln!(
        &mut out,
        "/// Kernel direct imports and interop dispatch entrypoints (ABI v{}).",
        manifest.manifest.abi_version
    )
    .unwrap();
    writeln!(&mut out, "pub const BUILTIN_SPECS: &[BuiltinFnSpec] = &[").unwrap();
    for (sym, params, returns) in specs {
        let returns = match returns {
            AbiReturnKind::Void => "AbiReturnKind::Void",
            AbiReturnKind::Ptr => "AbiReturnKind::Ptr",
            AbiReturnKind::I64 => "AbiReturnKind::I64",
            AbiReturnKind::I32 => "AbiReturnKind::I32",
            AbiReturnKind::F64 => "AbiReturnKind::F64",
            AbiReturnKind::Never => "AbiReturnKind::Never",
        };
        if params == "EMPTY" {
            writeln!(&mut out, "    BuiltinFnSpec {{ symbol: SYM_{sym}, params: &EMPTY, returns: {returns} }},")
                .unwrap();
        } else {
            writeln!(&mut out, "    BuiltinFnSpec {{ symbol: SYM_{sym}, params: &{params}, returns: {returns} }},")
                .unwrap();
        }
    }
    writeln!(&mut out, "];").unwrap();
    out
}

enum AbiParamKind {
    Ptr,
    I64,
    F64,
}

#[derive(Clone, Copy)]
enum AbiReturnKind {
    Void,
    Ptr,
    I64,
    I32,
    F64,
    Never,
}

fn manifest_param_kinds(params: &[String]) -> Vec<AbiParamKind> {
    params
        .iter()
        .map(|param| match param.as_str() {
            "ptr" | "string" => AbiParamKind::Ptr,
            "f64" => AbiParamKind::F64,
            _ => AbiParamKind::I64,
        })
        .collect()
}

fn manifest_return_abi(returns: &str) -> AbiReturnKind {
    match returns {
        "unit" | "void" => AbiReturnKind::Void,
        "ptr" => AbiReturnKind::Ptr,
        "never" => AbiReturnKind::Never,
        "i32" => AbiReturnKind::I32,
        "f64" => AbiReturnKind::F64,
        _ => AbiReturnKind::I64,
    }
}

fn param_array_name(
    params: &[AbiParamKind],
    seen: &mut HashMap<Vec<AbiParamKind>, String>,
    next_id: &mut usize,
) -> String {
    if let Some(name) = seen.get(params) {
        return name.clone();
    }
    let name = if params.is_empty() {
        "EMPTY".to_string()
    } else if params.len() == 2 && params[0] == AbiParamKind::Ptr && params[1] == AbiParamKind::Ptr {
        "PTR_PTR".to_string()
    } else if params.len() == 1 && params[0] == AbiParamKind::Ptr {
        "PTR_ONLY".to_string()
    } else if params.len() == 1 && params[0] == AbiParamKind::I64 {
        "I64_ONLY".to_string()
    } else {
        let generated = format!("PARAMS_{next_id}");
        *next_id += 1;
        generated
    };
    seen.insert(params.to_vec(), name.clone());
    name
}

fn format_param_array(params: &[AbiParamKind]) -> String {
    if params.is_empty() {
        return "[]".to_string();
    }
    let inner = params
        .iter()
        .map(|kind| match kind {
            AbiParamKind::Ptr => "AbiParamKind::Ptr",
            AbiParamKind::I64 => "AbiParamKind::I64",
            AbiParamKind::F64 => "AbiParamKind::F64",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}
