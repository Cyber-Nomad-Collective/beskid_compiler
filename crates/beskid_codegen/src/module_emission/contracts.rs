use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::analysis::diagnostics::make_diagnostic;
use beskid_analysis::services::SemanticDiagnosticsError;
use beskid_isle::{AstNodeKey, FunctionEmissionError, LoweringErrorKind};
use beskid_queries::{SemanticError, SemanticFinding};
use cranelift_module::ModuleError;

use crate::CodegenInput;

const INTERNAL_ERROR_HELP: &str = "this is an internal compiler error, please report it";

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
    /// requested items before specialization or ISLE lowering ran. `rendered` carries the coded
    /// message and source site of every finding of the pass, so a FAIL line reports every
    /// violation at once rather than stopping at the first; [`Self::into_report`] turns the
    /// findings into source-excerpt diagnostics.
    #[error("legality gate rejected the requested items:\n{rendered}")]
    Legality { rendered: String, findings: Vec<SemanticFinding> },
    /// A compiler gap after a clean legality gate: a semantic fact that is still unavailable
    /// (E2101) or a construct no ISLE rule or fact lowers (E2102). Never a user diagnostic code.
    #[error("syntax ISLE emission failed: {rendered}")]
    Internal { rendered: String, finding: SemanticFinding },
}

impl SyntaxModuleEmissionError {
    /// Convert into the `anyhow` error a host reports. The message is `"{context}: {self}"`;
    /// legality findings and internal errors additionally keep one structured diagnostic per
    /// finding, located in the owning unit's source, so `report_from_anyhow` renders code, label,
    /// help, and source excerpt. When a finding's source cannot be located the error keeps only
    /// its message, which already names every site.
    pub fn into_report(self, input: &CodegenInput<'_>, context: &str) -> anyhow::Error {
        let message = format!("{context}: {self}");
        let findings = match self {
            Self::Legality { findings, .. } => findings,
            Self::Internal { finding, .. } => vec![finding],
            Self::Module(_) | Self::Emission(_) | Self::DuplicateSymbol(_) => Vec::new(),
        };
        let diagnostics = findings.iter().map(|finding| finding_diagnostic(input, finding)).collect::<Option<Vec<_>>>();
        match diagnostics {
            Some(diagnostics) if !diagnostics.is_empty() => {
                anyhow::Error::new(SemanticDiagnosticsError::from_diagnostics(diagnostics)).context(message)
            }
            _ => anyhow::anyhow!(message),
        }
    }
}

/// Render `finding` as a diagnostic against its unit's source in the input assembly.
fn finding_diagnostic(
    input: &CodegenInput<'_>,
    finding: &SemanticFinding,
) -> Option<beskid_analysis::analysis::SemanticDiagnostic> {
    let db = input.database();
    let path = finding.site.unit.path(db);
    let unit = input.typed_program().assembly.units.iter().find(|unit| {
        unit.path == *path
            || std::fs::canonicalize(&unit.path).ok().zip(std::fs::canonicalize(path).ok()).is_some_and(|(a, b)| a == b)
    })?;
    let span = beskid_queries::node_span(db, finding.site).ok().flatten()?;
    let kind = &finding.kind;
    Some(make_diagnostic(
        &unit.logical_name,
        &unit.source,
        span,
        kind.message(),
        kind.label(),
        kind.help(),
        Some(kind.code().to_string()),
        kind.severity(),
    ))
}

pub(super) fn emission_error(input: &CodegenInput<'_>, error: FunctionEmissionError) -> SyntaxModuleEmissionError {
    let db = input.database();
    if let FunctionEmissionError::Lowering(lowering) = &error
        && lowering.kind() == LoweringErrorKind::MissingRuleOrFact
    {
        let construct = beskid_queries::node_kind(db, lowering.key())
            .ok()
            .flatten()
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| "Unknown".to_owned());
        return internal_error(
            db,
            SemanticIssueKind::InternalLoweringRuleMissing { construct },
            lowering.key(),
            &error.display_with_db(db),
        );
    }
    SyntaxModuleEmissionError::Emission(error.display_with_db(db))
}

pub(super) fn emission_verification(message: impl Into<String>) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(format!("Verification({})", message.into()))
}

/// Classify a semantic query error met while resolving module items for `site`. An unavailable
/// fact is a compiler gap after the legality gate has passed, reported as internal error E2101 at
/// the site the query named (or `site`); any other error keeps its verification message.
pub(super) fn semantic_fact_error(
    db: &dyn beskid_queries::Db,
    site: AstNodeKey,
    context: impl std::fmt::Display,
    error: SemanticError,
) -> SyntaxModuleEmissionError {
    match error.unavailable_query() {
        Some(query) => internal_error(
            db,
            SemanticIssueKind::InternalSemanticFactUnavailable { query: query.to_owned() },
            error.unavailable_site().unwrap_or(site),
            &format!("{context}: {error}"),
        ),
        None => emission_verification(format!("{context}: {error}")),
    }
}

fn internal_error(
    db: &dyn beskid_queries::Db,
    kind: SemanticIssueKind,
    site: AstNodeKey,
    detail: &str,
) -> SyntaxModuleEmissionError {
    let rendered = format!(
        "{} {} at {} ({detail}); {INTERNAL_ERROR_HELP}",
        kind.code(),
        kind.message(),
        beskid_queries::format_ast_node_site(db, site)
    );
    SyntaxModuleEmissionError::Internal { rendered, finding: SemanticFinding { kind, site, related: Vec::new() } }
}

/// Render every finding of a legality gate pass as one FAIL-line-ready error: one `<code>
/// <message> at <site>` line per finding, in the gate's own order. The site uses the same
/// `format_ast_node_site` label/construct/range shape every other FAIL line in this crate already
/// carries, so a legality rejection is diagnosable the same way a `FunctionEmissionError` is.
pub(super) fn legality_error(db: &dyn beskid_queries::Db, findings: Vec<SemanticFinding>) -> SyntaxModuleEmissionError {
    let rendered = findings
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
        .join("\n");
    SyntaxModuleEmissionError::Legality { rendered, findings }
}
