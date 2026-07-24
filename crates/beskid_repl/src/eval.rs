use anyhow::Result;
use beskid_engine::Engine;
use beskid_engine::services::{
    prepare_syntax_front_end, run_entrypoint_from_front_end_with_engine, syntax_entrypoint_return_type_from_front_end,
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
                Ok(return_type) => EvalOutcome::Type(format_semantic_type(return_type)),
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
        return Ok(WrappedSnippet { source: format!("unit Main() {{ {trimmed} }}"), return_type: "unit".to_string() });
    }

    for ret in EXPR_RETURN_TYPES {
        let wrapped = format!("{ret} Main() {{ return {trimmed}; }}");
        if prepare_wrapped(&wrapped).is_ok() {
            return Ok(WrappedSnippet { source: wrapped, return_type: (*ret).to_string() });
        }
    }

    Err(format!("could not type-check expression `{trimmed}` (tried return types: {})", EXPR_RETURN_TYPES.join(", ")))
}

fn is_likely_statement(snippet: &str) -> bool {
    let trimmed = snippet.trim();
    if trimmed.ends_with(';') {
        return true;
    }

    const STATEMENT_PREFIXES: &[&str] =
        &["let ", "mut ", "for ", "while ", "if ", "return ", "unit ", "spawn ", "match "];
    STATEMENT_PREFIXES.iter().any(|prefix| trimmed.starts_with(prefix))
}

fn prepare_wrapped(source: &str) -> Result<beskid_analysis::services::FrontEndTypedResult, String> {
    prepare_syntax_front_end(std::path::Path::new(REPL_SOURCE_PATH), source).map_err(format_lower_error)
}

fn run_wrapped(
    engine: &mut Engine,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String, String> {
    let front = prepare_wrapped(source)?;
    run_entrypoint_from_front_end_with_engine(engine, &front, REPL_SOURCE_PATH, source, entrypoint, pipeline)
        .map_err(format_lower_error)
}

fn format_semantic_type(ty: beskid_queries::SemanticTypeId) -> String {
    ty.display_name()
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

    fn shared_exact_kit_prefix() -> &'static std::path::Path {
        use std::sync::OnceLock;
        static PREFIX: OnceLock<std::path::PathBuf> = OnceLock::new();
        PREFIX.get_or_init(|| {
            let prefix = tempfile::tempdir().expect("exact kit prefix").keep();
            build_native_host(prefix.clone(), RuntimeKitProfile::Debug).expect("publish exact native kit");
            prefix
        })
    }

    fn exact_kit_engine() -> Engine {
        let target = host_runtime_target().expect("supported native host target");
        Engine::with_runtime_kit(shared_exact_kit_prefix(), target, BuildProfile::Debug).expect("load exact kit")
    }

    #[test]
    fn eval_i64_expression() {
        let mut engine = exact_kit_engine();
        let outcome = eval_snippet(&mut engine, "41 + 1");
        assert_eq!(outcome, EvalOutcome::Value("42".to_string()));
    }

    #[test]
    fn eval_uses_a_fresh_native_runtime_kit() {
        let mut engine = exact_kit_engine();
        assert_eq!(eval_snippet(&mut engine, "true"), EvalOutcome::Value("true".to_string()));
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[ignore = "requires the staged Linux native runtime-kit prefix"]
    fn staged_linux_runtime_kit_evaluates_a_snippet() {
        let prefix = std::env::var_os("BESKID_RUNTIME_PREFIX")
            .map(std::path::PathBuf::from)
            .expect("Linux evidence must set BESKID_RUNTIME_PREFIX");
        let profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE").as_deref() {
            Ok("debug") => BuildProfile::Debug,
            Ok("release") => BuildProfile::Release,
            value => panic!("unsupported staged runtime profile: {value:?}"),
        };
        let target = host_runtime_target().expect("supported native host target");
        let mut engine = Engine::with_runtime_kit(&prefix, target, profile).expect("load the staged Linux runtime kit");

        assert_eq!(eval_snippet(&mut engine, "41 + 1"), EvalOutcome::Value("42".to_string()));
    }

    #[test]
    fn eval_unit_statement() {
        let mut engine = exact_kit_engine();
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
        let mut engine = exact_kit_engine();
        assert_eq!(eval_snippet(&mut engine, "10 + 5"), EvalOutcome::Value("15".to_string()));
        assert_eq!(eval_snippet(&mut engine, "6 * 7"), EvalOutcome::Value("42".to_string()));
    }

    #[test]
    fn eval_reports_type_error() {
        let mut engine = exact_kit_engine();
        let outcome = eval_snippet(&mut engine, "let x = true + 1;");
        assert!(matches!(outcome, EvalOutcome::Error(_)));
    }

    #[test]
    fn formats_word_type_from_syntax_authority() {
        assert_eq!(format_semantic_type(beskid_queries::SemanticTypeId::WORD), "word");
    }

    #[test]
    fn repl_session_fails_closed_when_exact_kit_manifest_is_missing() {
        let empty = tempfile::tempdir().expect("empty prefix");
        let previous = std::env::var_os("BESKID_RUNTIME_PREFIX");
        // SAFETY: this integration target serializes around the process environment and restores it.
        unsafe { std::env::set_var("BESKID_RUNTIME_PREFIX", empty.path()) };
        let error = match crate::session::ReplSession::try_new() {
            Ok(_) => panic!("missing exact kit must fail closed for REPL"),
            Err(error) => error,
        };
        unsafe {
            if let Some(value) = previous {
                std::env::set_var("BESKID_RUNTIME_PREFIX", value);
            } else {
                std::env::remove_var("BESKID_RUNTIME_PREFIX");
            }
        }
        let message = error.to_string();
        assert!(
            message.contains("abi.json") || message.contains("MetadataRead") || message.contains("runtime kit"),
            "expected missing-kit fail-closed diagnostic, got {message}"
        );
    }

    #[test]
    fn eval_fails_closed_when_exact_kit_is_tampered() {
        let Ok(target) = host_runtime_target() else {
            return;
        };
        let prefix = tempfile::tempdir().expect("tampered kit prefix");
        let built = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
            .expect("publish canonical native runtime kit");
        std::fs::write(&built.shared_library, b"tampered shared runtime").expect("tamper shared library");

        let error = match Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug) {
            Ok(_) => panic!("tampered exact kit must fail closed for REPL Engine"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(
            message.contains("runtime kit")
                || message.contains("hash")
                || message.contains("validation")
                || message.contains("ArtifactHash"),
            "expected tampered-kit fail-closed diagnostic, got {message}"
        );
    }
}
