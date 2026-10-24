use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::analysis::diagnostic_kinds::issue_diagnostic;
use beskid_analysis::services::SemanticDiagnosticsError;
use beskid_isle::{AstNodeKey, FunctionEmissionError, LoweringErrorKind};
use beskid_queries::{SemanticError, SemanticFinding};
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
    /// A retained `[Serialize]` target reaches an application without compiler-issued field codec
    /// facts (`beskid_queries::serialization_field_codec_admission`); rejected before lowering.
    #[error("serialization field codec facts are missing: {0}")]
    SerializationFacts(String),
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
    /// every variant additionally keeps structured diagnostics, so `report_from_anyhow` renders
    /// code, label, help, and source excerpt and no raw internal text reaches the user:
    ///
    /// * legality findings and sited internal errors render at their site in the owning unit;
    /// * an unsited failure (a module error, a duplicate symbol, missing serialization facts, a
    ///   verification failure without a construct) is internal error E2103/E2105 anchored at the
    ///   start of the entry unit, the program being compiled.
    ///
    /// A finding whose site cannot be located falls back to that program-level anchor too.
    pub fn into_report(self, input: &CodegenInput<'_>, context: &str) -> anyhow::Error {
        let message = format!("{context}: {self}");
        let diagnostics = match self {
            Self::Legality { findings, .. } => findings
                .iter()
                .map(|finding| {
                    finding_diagnostic(input, finding)
                        .or_else(|| program_diagnostic(input, &finding.kind))
                })
                .collect::<Option<Vec<_>>>(),
            Self::Internal { finding, .. } => {
                finding_diagnostic(input, &finding).or_else(|| program_diagnostic(input, &finding.kind)).map(|d| vec![d])
            }
            Self::Emission(reason) => program_diagnostic(
                input,
                &SemanticIssueKind::InternalLoweringInvariant { construct: "Program".to_owned(), reason },
            )
            .map(|diagnostic| vec![diagnostic]),
            Self::SerializationFacts(reason) => program_diagnostic(
                input,
                &SemanticIssueKind::InternalLoweringInvariant {
                    construct: "Program".to_owned(),
                    reason: format!("serialization field codec facts are missing: {reason}"),
                },
            )
            .map(|diagnostic| vec![diagnostic]),
            Self::DuplicateSymbol(symbol) => program_diagnostic(
                input,
                &SemanticIssueKind::InternalBuildFailure { reason: format!("duplicate symbol `{symbol}` in the module") },
            )
            .map(|diagnostic| vec![diagnostic]),
            Self::Module(error) => program_diagnostic(
                input,
                &SemanticIssueKind::InternalBuildFailure { reason: format!("module declaration failed: {error}") },
            )
            .map(|diagnostic| vec![diagnostic]),
        };
        match diagnostics {
            Some(diagnostics) if !diagnostics.is_empty() => {
                anyhow::Error::new(SemanticDiagnosticsError::from_diagnostics(diagnostics)).context(message)
            }
            _ => anyhow::anyhow!(message),
        }
    }
}

/// `kind` anchored at the start of the entry unit: the program being compiled, for a failure that
/// has no construct of its own.
fn program_diagnostic(
    input: &CodegenInput<'_>,
    kind: &SemanticIssueKind,
) -> Option<beskid_analysis::analysis::SemanticDiagnostic> {
    let assembly = &input.typed_program().assembly;
    let unit = assembly.units.get(assembly.entry_index)?;
    let start = beskid_analysis::syntax::SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) };
    let mut diagnostic = issue_diagnostic(&unit.logical_name, &unit.source, start, kind);
    diagnostic.label = "while compiling this program".to_owned();
    Some(diagnostic)
}

/// The assembly unit that owns `site`.
fn site_unit<'a>(input: &'a CodegenInput<'_>, site: AstNodeKey) -> Option<&'a beskid_analysis::projects::SourceUnit> {
    let path = site.unit.path(input.database());
    input.typed_program().assembly.units.iter().find(|unit| {
        unit.path == *path
            || std::fs::canonicalize(&unit.path).ok().zip(std::fs::canonicalize(path).ok()).is_some_and(|(a, b)| a == b)
    })
}

/// Render `finding` as a diagnostic against its unit's source in the input assembly, with every
/// related site it names (a site that cannot be located is left out).
fn finding_diagnostic(
    input: &CodegenInput<'_>,
    finding: &SemanticFinding,
) -> Option<beskid_analysis::analysis::SemanticDiagnostic> {
    let db = input.database();
    let unit = site_unit(input, finding.site)?;
    let span = beskid_queries::node_span(db, finding.site).ok().flatten()?;
    let mut diagnostic = issue_diagnostic(&unit.logical_name, &unit.source, span, &finding.kind);
    for (site, label) in &finding.related {
        let (Some(related_unit), Some(related_span)) =
            (site_unit(input, *site), beskid_queries::node_span(db, *site).ok().flatten())
        else {
            continue;
        };
        diagnostic = diagnostic.with_related_in(
            related_unit.logical_name.clone(),
            related_unit.source.clone(),
            related_span,
            *label,
        );
    }
    Some(diagnostic)
}

/// The syntax node kind of `key` (`AssignExpression`), or `Unknown`.
fn construct_of(db: &dyn beskid_queries::Db, key: AstNodeKey) -> String {
    beskid_queries::node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "Unknown".to_owned())
}

/// Classify a function emission failure. A missing rule is internal error E2102, a rejected
/// user `clif {}` block is the user error E1232 at the block, and every other lowering kind is an
/// internal invariant (E2103) at its construct; all carry the site, never only text.
pub(super) fn emission_error(input: &CodegenInput<'_>, error: FunctionEmissionError) -> SyntaxModuleEmissionError {
    let db = input.database();
    let lowering = match &error {
        FunctionEmissionError::Lowering(lowering) => lowering,
        FunctionEmissionError::Verification { site, message } => {
            return internal_error(
                db,
                SemanticIssueKind::InternalLoweringInvariant {
                    construct: construct_of(db, *site),
                    reason: format!("the generated function failed verification: {message}"),
                },
                *site,
                &error.display_with_db(db),
            );
        }
    };
    let key = lowering.key();
    let construct = construct_of(db, key);
    let kind = match lowering.kind() {
        LoweringErrorKind::MissingRuleOrFact => SemanticIssueKind::InternalLoweringRuleMissing { construct },
        LoweringErrorKind::InvalidClifBlock(detail) => {
            return legality_error(
                db,
                vec![SemanticFinding { kind: SemanticIssueKind::InvalidClifBlock { detail }, site: key, related: Vec::new() }],
            );
        }
        other => SemanticIssueKind::InternalLoweringInvariant { construct, reason: lowering_reason(&other) },
    };
    internal_error(db, kind, key, &error.display_with_db(db))
}

/// A lowering error kind in words, for the internal reason of an E2103 diagnostic.
fn lowering_reason(kind: &LoweringErrorKind) -> String {
    match kind {
        LoweringErrorKind::MissingRuleOrFact => "no lowering rule or fact matches this construct".to_owned(),
        LoweringErrorKind::StringMaterialization(error) => format!("a string value could not be materialized ({error})"),
        LoweringErrorKind::UnknownCallee(_) => "the call target has no lowering".to_owned(),
        LoweringErrorKind::InvalidPrimitiveNumericConversion(reason) => {
            format!("invalid primitive numeric conversion ({reason})")
        }
        LoweringErrorKind::InvalidArrayLayout => "invalid array layout".to_owned(),
        LoweringErrorKind::UnprovenCollectionOwner => "the owner of a collection is not proven".to_owned(),
        LoweringErrorKind::MissingManagedReferenceFact => "a store target has no managed-reference fact".to_owned(),
        LoweringErrorKind::InvalidStructLayout => "invalid type layout".to_owned(),
        LoweringErrorKind::InvalidStructField(index) => format!("invalid field index {index}"),
        LoweringErrorKind::InvalidDestructiveRead => "invalid destructive read of a field".to_owned(),
        LoweringErrorKind::InvalidExchange => "invalid `exchange`".to_owned(),
        LoweringErrorKind::InvalidEnumLayout => "invalid enum layout".to_owned(),
        LoweringErrorKind::InvalidEnumVariant(index) => format!("invalid enum variant index {index}"),
        LoweringErrorKind::InvalidMatchArms => "the match arms do not fit the scrutinee layout".to_owned(),
        LoweringErrorKind::NonExhaustiveMatch => "the match is not exhaustive at lowering".to_owned(),
        LoweringErrorKind::InvalidBlockExpression => "invalid block expression".to_owned(),
        LoweringErrorKind::InvalidRangeFor => "invalid range `for` loop".to_owned(),
        LoweringErrorKind::InvalidArrayFor => "invalid array `for` loop".to_owned(),
        LoweringErrorKind::InvalidIteratorFor => "invalid iterator `for` loop".to_owned(),
        LoweringErrorKind::InvalidClifBlock(detail) => format!("invalid clif block ({detail})"),
    }
}

/// A verification failure without a construct. It renders as internal error E2103 anchored at the
/// program being compiled ([`SyntaxModuleEmissionError::into_report`]).
pub(super) fn emission_verification(message: impl Into<String>) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(message.into())
}

/// A failed fact of the construct `key`: internal error E2103 at that construct.
pub(super) fn emission_verification_at(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    error: impl std::fmt::Display,
) -> SyntaxModuleEmissionError {
    let reason = error.to_string();
    internal_error(
        db,
        SemanticIssueKind::InternalLoweringInvariant { construct: construct_of(db, key), reason: reason.clone() },
        key,
        &reason,
    )
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
    let at = error.unavailable_site().unwrap_or(site);
    let construct = construct_of(db, at);
    let detail = format!("{context}: {error}");
    match error.unavailable_query() {
        Some(query) => internal_error(
            db,
            SemanticIssueKind::InternalSemanticFactUnavailable { query: query.to_owned(), construct },
            at,
            &detail,
        ),
        None => internal_error(
            db,
            SemanticIssueKind::InternalLoweringInvariant { construct, reason: detail.clone() },
            at,
            &detail,
        ),
    }
}

fn internal_error(
    db: &dyn beskid_queries::Db,
    kind: SemanticIssueKind,
    site: AstNodeKey,
    detail: &str,
) -> SyntaxModuleEmissionError {
    let rendered =
        format!("{} {} at {} ({detail})", kind.code(), kind.message(), beskid_queries::format_ast_node_site(db, site));
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
