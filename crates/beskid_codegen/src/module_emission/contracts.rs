use beskid_isle::FunctionEmissionError;
use cranelift_module::ModuleError;

use crate::CodegenInput;

#[derive(Debug, thiserror::Error)]
pub enum SyntaxModuleEmissionError {
    #[error("module declaration failed: {0}")]
    Module(#[from] ModuleError),
    /// Pre-formatted with [`FunctionEmissionError::display_with_db`] so FAIL lines include
    /// construct and source range, not only `#gN:nN`.
    #[error("syntax ISLE emission failed: {0}")]
    Emission(String),
    #[error("syntax module declares duplicate symbol `{0}`")]
    DuplicateSymbol(String),
    /// The reachability-scoped semantic legality gate
    /// (`beskid_queries::semantic_contract::legality::check_items`) rejected one or more of the
    /// requested items before specialization or ISLE lowering ran. Pre-formatted with the coded
    /// message and source site of every finding of the pass, so a FAIL line reports every
    /// violation at once rather than stopping at the first.
    #[error("legality gate rejected the requested items:\n{0}")]
    Legality(String),
}

pub(super) fn emission_error(input: &CodegenInput<'_>, error: FunctionEmissionError) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(error.display_with_db(input.database()))
}

pub(super) fn emission_verification(message: impl Into<String>) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(format!("Verification({})", message.into()))
}

/// Render every finding of a legality gate pass as one FAIL-line-ready string: one `<code>
/// <message> at <site>` line per finding, in the gate's own order. The site uses the same
/// `format_ast_node_site` label/construct/range shape every other FAIL line in this crate already
/// carries, so a legality rejection is diagnosable the same way a `FunctionEmissionError` is.
pub(super) fn render_legality_findings(
    db: &dyn beskid_queries::Db,
    findings: &[beskid_queries::SemanticFinding],
) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{} {} at {}",
                finding.kind.code(),
                finding.kind.message(),
                beskid_queries::format_ast_node_site(db, finding.site)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
