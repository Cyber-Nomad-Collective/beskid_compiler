use anyhow::Result;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_engine::Engine;
use beskid_engine::services::{
    run_entrypoint_from_front_end_with_engine, syntax_entrypoint_return_type_from_front_end,
};
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
        Ok(wrapped) => match run_wrapped(engine, &wrapped.source, "Main", None) {
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
        Ok(wrapped) => match prepare_wrapped(&wrapped.source) {
            Ok(front) => match syntax_entrypoint_return_type_from_front_end(&front, "Main") {
                Ok(return_type) => EvalOutcome::Type(format_semantic_type(return_type).to_owned()),
                Err(error) => EvalOutcome::Error(error.to_string()),
            },
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
            source: format!("unit Main() {{ {trimmed} }}"),
            return_type: "unit".to_string(),
        });
    }

    for ret in EXPR_RETURN_TYPES {
        let wrapped = format!("{ret} Main() {{ return {trimmed}; }}");
        if prepare_wrapped(&wrapped).is_ok() {
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

fn prepare_wrapped(source: &str) -> Result<beskid_analysis::services::FrontEndTypedResult, String> {
    let source_path = beskid_codegen::materialize_source_path_for_lowering(
        std::path::Path::new(REPL_SOURCE_PATH),
        source,
    )
    .map_err(format_lower_error)?;
    let plan = synthetic_compile_plan_for_source(&source_path);
    let resolved: ResolvedInput =
        resolved_input_from_plan(source_path, source.to_owned(), plan, None, None);
    beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions::default(),
        None,
    )
    .map_err(format_lower_error)
}

fn run_wrapped(
    engine: &mut Engine,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String, String> {
    let front = prepare_wrapped(source)?;
    run_entrypoint_from_front_end_with_engine(
        engine,
        &front,
        REPL_SOURCE_PATH,
        source,
        entrypoint,
        pipeline,
    )
    .map_err(format_lower_error)
}

fn format_semantic_type(ty: beskid_queries::SemanticTypeId) -> &'static str {
    match ty {
        beskid_queries::SemanticTypeId::UNIT => "unit",
        beskid_queries::SemanticTypeId::BOOL => "bool",
        beskid_queries::SemanticTypeId::I32 => "i32",
        beskid_queries::SemanticTypeId::I64 => "i64",
        beskid_queries::SemanticTypeId::U8 => "u8",
        beskid_queries::SemanticTypeId::F64 => "f64",
        beskid_queries::SemanticTypeId::CHAR => "char",
        beskid_queries::SemanticTypeId::STRING => "string",
        beskid_queries::SemanticTypeId::WORD => "word",
        beskid_queries::SemanticTypeId::POINTER => "pointer",
        beskid_queries::SemanticTypeId::NEVER => "never",
        _ => "unknown",
    }
}

fn format_lower_error(error: anyhow::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_abi::runtime_kit::BuildProfile;
    use beskid_engine::host_runtime_target;
    use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

    #[test]
    fn wraps_expression_as_i64_main() {
        let wrapped = wrap_snippet("1 + 1").expect("wrap");
        assert!(wrapped.source.contains("i64 Main()"));
        assert_eq!(wrapped.return_type, "i64");
    }

    #[test]
    fn wraps_statement_as_unit_main() {
        let wrapped = wrap_snippet("let x = 1;").expect("wrap");
        assert!(wrapped.source.starts_with("unit Main()"));
        assert_eq!(wrapped.return_type, "unit");
    }

    #[test]
    fn eval_i64_expression() {
        let mut engine = Engine::new();
        let outcome = eval_snippet(&mut engine, "41 + 1");
        assert_eq!(outcome, EvalOutcome::Value("42".to_string()));
    }

    #[test]
    fn eval_uses_a_fresh_native_runtime_kit() {
        let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
        build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
            .expect("publish canonical native runtime kit");
        let target = host_runtime_target().expect("supported native host target");
        let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug)
            .expect("load the exact fresh runtime kit");

        assert_eq!(
            eval_snippet(&mut engine, "true"),
            EvalOutcome::Value("true".to_string())
        );
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

    #[test]
    fn formats_word_type_from_syntax_authority() {
        assert_eq!(
            format_semantic_type(beskid_queries::SemanticTypeId::WORD),
            "word"
        );
    }
}
