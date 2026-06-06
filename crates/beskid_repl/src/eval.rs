use std::path::Path;

use anyhow::Result;
use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::resolve::ItemKind;
use beskid_analysis::types::{TypeInfo, format_type_id};
use beskid_codegen::services::lower_source;
use beskid_engine::Engine;
use beskid_pipeline::PipelineObserver;

use crate::REPL_SOURCE_PATH;

/// Outcome of evaluating or typing a snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalOutcome {
    Value(String),
    Unit,
    Type(String),
    Error(String),
}

const EXPR_RETURN_TYPES: &[&str] = &["i64", "i32", "bool", "string", "f64", "char"];

pub fn eval_snippet(engine: &mut Engine, snippet: &str) -> EvalOutcome {
    match wrap_snippet(snippet) {
        Ok(wrapped) => match run_wrapped(engine, &wrapped.source, "main", None) {
            Ok(output) => {
                if wrapped.return_type == "unit" {
                    EvalOutcome::Unit
                } else {
                    EvalOutcome::Value(output)
                }
            }
            Err(error) => EvalOutcome::Error(error),
        },
        Err(error) => EvalOutcome::Error(error),
    }
}

pub fn type_of_snippet(snippet: &str) -> EvalOutcome {
    match wrap_snippet(snippet) {
        Ok(wrapped) => match lower_wrapped(&wrapped.source) {
            Ok(lowered) => {
                let Some(main) = find_entrypoint(&lowered.resolution, "main") else {
                    return EvalOutcome::Error("missing `main` entrypoint".to_string());
                };
                let Some(signature) = lowered.typed.function_signatures.get(&main.id) else {
                    return EvalOutcome::Error("missing signature for `main`".to_string());
                };
                let display = format_type_id(
                    &lowered.typed,
                    Some(&lowered.resolution),
                    signature.return_type,
                );
                EvalOutcome::Type(display)
            }
            Err(error) => EvalOutcome::Error(error),
        },
        Err(error) => EvalOutcome::Error(error),
    }
}

struct WrappedSnippet {
    source: String,
    return_type: String,
}

fn wrap_snippet(snippet: &str) -> Result<WrappedSnippet, String> {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Err("empty input".to_string());
    }

    if is_likely_statement(trimmed) {
        return Ok(WrappedSnippet {
            source: format!("unit main() {{ {trimmed} }}"),
            return_type: "unit".to_string(),
        });
    }

    for ret in EXPR_RETURN_TYPES {
        let wrapped = format!("{ret} main() {{ return {trimmed}; }}");
        if lower_wrapped(&wrapped).is_ok() {
            return Ok(WrappedSnippet {
                source: wrapped,
                return_type: (*ret).to_string(),
            });
        }
    }

    Err(format!(
        "could not type-check expression `{trimmed}` (tried return types: {})",
        EXPR_RETURN_TYPES.join(", ")
    ))
}

fn is_likely_statement(snippet: &str) -> bool {
    let trimmed = snippet.trim();
    if trimmed.ends_with(';') {
        return true;
    }

    const STATEMENT_PREFIXES: &[&str] = &[
        "let ", "mut ", "for ", "while ", "if ", "return ", "unit ", "spawn ", "match ",
    ];
    STATEMENT_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

fn lower_wrapped(source: &str) -> Result<beskid_codegen::services::LoweredProgram, String> {
    lower_source(
        Path::new(REPL_SOURCE_PATH),
        source,
        true,
    )
    .map_err(format_lower_error)
}

fn run_wrapped(
    engine: &mut Engine,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String, String> {
    let lowered = lower_wrapped(source)?;
    engine
        .compile_artifact_with_pipeline(&lowered.artifact, pipeline)
        .map_err(|err| format!("JIT compile failed: {err}"))?;

    let entrypoint_info = find_entrypoint(&lowered.resolution, entrypoint)
        .ok_or_else(|| format!("missing entrypoint `{entrypoint}`"))?;

    let signature = lowered
        .typed
        .function_signatures
        .get(&entrypoint_info.id)
        .ok_or_else(|| format!("missing signature for `{entrypoint}`"))?;

    if !signature.params.is_empty() {
        return Err(format!("entrypoint `{entrypoint}` must take no parameters"));
    }

    let return_info = lowered
        .typed
        .types
        .get(signature.return_type)
        .ok_or_else(|| format!("missing return type for `{entrypoint}`"))?;

    let jit_symbol = entrypoint_info.name.clone();
    let ptr = unsafe { engine.entrypoint_ptr(&jit_symbol) }
        .map_err(|err| format!("entrypoint lookup failed: {err}"))?;
    if ptr.is_null() {
        return Err(format!("entrypoint `{entrypoint}` returned null pointer"));
    }

    Ok(engine.with_runtime(|_, _| format_return_value(ptr, return_info)))
}

fn find_entrypoint<'a>(
    resolution: &'a beskid_analysis::resolve::Resolution,
    entrypoint: &str,
) -> Option<&'a beskid_analysis::resolve::ItemInfo> {
    resolution.items.iter().find(|item| {
        entrypoint_matches_item(item, entrypoint)
            && (item.kind == ItemKind::Function || item.kind == ItemKind::Test)
    })
}

fn entrypoint_matches_item(item: &beskid_analysis::resolve::ItemInfo, entrypoint: &str) -> bool {
    if item.name == entrypoint {
        return true;
    }
    if !entrypoint.contains("::") {
        return false;
    }
    entrypoint.rsplit("::").next() == Some(item.name.as_str())
}

fn format_return_value(ptr: *const u8, return_info: &TypeInfo) -> String {
    match return_info {
        TypeInfo::Primitive(HirPrimitiveType::Unit) => "ok".to_owned(),
        TypeInfo::Primitive(HirPrimitiveType::Never) => {
            let callable: extern "C" fn() -> ! = unsafe { std::mem::transmute(ptr) };
            callable()
        }
        TypeInfo::Primitive(HirPrimitiveType::String)
        | TypeInfo::Named(_)
        | TypeInfo::GenericParam(_)
        | TypeInfo::Applied { .. }
        | TypeInfo::Function { .. }
        | TypeInfo::Array(_)
        | TypeInfo::Fiber(_) => {
            let value: u64 = unsafe { invoke0(ptr) };
            format!("0x{value:016x}")
        }
        TypeInfo::Primitive(HirPrimitiveType::I64) => unsafe { invoke0::<i64>(ptr) }.to_string(),
        TypeInfo::Primitive(HirPrimitiveType::I32) => unsafe { invoke0::<i32>(ptr) }.to_string(),
        TypeInfo::Primitive(HirPrimitiveType::U8) => unsafe { invoke0::<u8>(ptr) }.to_string(),
        TypeInfo::Primitive(HirPrimitiveType::Bool) => {
            (unsafe { invoke0::<u8>(ptr) } != 0).to_string()
        }
        TypeInfo::Primitive(HirPrimitiveType::F64) => unsafe { invoke0::<f64>(ptr) }.to_string(),
        TypeInfo::Primitive(HirPrimitiveType::Char) => {
            let value: u32 = unsafe { invoke0(ptr) };
            std::char::from_u32(value).unwrap_or('\u{FFFD}').to_string()
        }
    }
}

unsafe fn invoke0<R>(ptr: *const u8) -> R {
    let callable: extern "C" fn() -> R = unsafe { std::mem::transmute(ptr) };
    callable()
}

fn format_lower_error(error: anyhow::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_expression_as_i64_main() {
        let wrapped = wrap_snippet("1 + 1").expect("wrap");
        assert!(wrapped.source.contains("i64 main()"));
        assert_eq!(wrapped.return_type, "i64");
    }

    #[test]
    fn wraps_statement_as_unit_main() {
        let wrapped = wrap_snippet("let x = 1;").expect("wrap");
        assert!(wrapped.source.starts_with("unit main()"));
        assert_eq!(wrapped.return_type, "unit");
    }

    #[test]
    fn eval_i64_expression() {
        let mut engine = Engine::new();
        let outcome = eval_snippet(&mut engine, "41 + 1");
        assert_eq!(outcome, EvalOutcome::Value("42".to_string()));
    }

    #[test]
    fn eval_unit_statement() {
        let mut engine = Engine::new();
        let outcome = eval_snippet(&mut engine, "let x = 1;");
        assert_eq!(outcome, EvalOutcome::Unit);
    }

    #[test]
    fn type_of_expression() {
        let outcome = type_of_snippet("1 + 1");
        assert_eq!(outcome, EvalOutcome::Type("i64".to_string()));
    }

    #[test]
    fn eval_reuses_engine() {
        let mut engine = Engine::new();
        assert_eq!(
            eval_snippet(&mut engine, "10 + 5"),
            EvalOutcome::Value("15".to_string())
        );
        assert_eq!(
            eval_snippet(&mut engine, "6 * 7"),
            EvalOutcome::Value("42".to_string())
        );
    }

    #[test]
    fn eval_reports_type_error() {
        let mut engine = Engine::new();
        let outcome = eval_snippet(&mut engine, "let x = true + 1;");
        assert!(matches!(outcome, EvalOutcome::Error(_)));
    }
}
