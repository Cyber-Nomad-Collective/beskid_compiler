//! Parse Beskid source to a spanned [`Program`](crate::syntax::Program), surfacing
//! [`MietteReportError`] on failure; a failed parse carries one structured parse diagnostic.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, anyhow};
use pest::Parser;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::analysis::diagnostics::{MietteReportError, Severity, make_diagnostic};
use crate::parser::{BeskidParser, Rule};
use crate::parsing::diagnose::{EngineRepair, codes, diagnose_parse_failure, refine_with_engine_repair};
use crate::parsing::parsable::Parsable;
use crate::syntax::{Program, Spanned};

use super::diagnostics_emit::parse_error_diagnostic;
use super::parse_recovery::recover_with_repair_candidates;

/// A parsed program and the parse errors found on the way.
///
/// When the source does not parse, `diagnostics` holds the error (with a fix where the repair has
/// one meaning) and `program` is the tree of the repaired source; `recovered` is then `true`.
#[derive(Debug, Clone)]
pub struct ParsedProgram {
    pub program: Spanned<Program>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub recovered: bool,
}

/// Parse in-memory source named `"<memory>"` for stack traces and diagnostics.
pub fn parse_program(source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name_and_diagnostics("<memory>", source).map(|parsed| parsed.program)
}

/// Parse with a stable `source_name` (file path or synthetic label) for diagnostics.
pub fn parse_program_with_source_name(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name_and_diagnostics(source_name, source).map(|parsed| parsed.program)
}

/// What recovering a source that does not parse came to: the repaired source the tree is built
/// from and the parse diagnostic, or the diagnostic alone when no repair parsed. Remembered per
/// source (see [`RECOVERY_MEMO`]): recovery tries up to [`super::parse_recovery`]'s candidate
/// budget of whole-file parses, which for a large file costs seconds, and an editor parses the
/// same broken text several times (outline, facts, completion) before the next keystroke.
#[derive(Clone)]
struct RecoveryOutcome {
    repaired_source: Option<String>,
    diagnostic: SemanticDiagnostic,
}

/// Recovery outcomes of the last [`RECOVERY_MEMO_CAPACITY`] distinct sources, newest last.
static RECOVERY_MEMO: OnceLock<Mutex<VecDeque<(u64, RecoveryOutcome)>>> = OnceLock::new();
const RECOVERY_MEMO_CAPACITY: usize = 64;
/// Keys of the recoveries run from scratch (candidate parses), for tests of the memo.
#[cfg(test)]
static RECOVERED_KEYS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

fn recovery_memo() -> &'static Mutex<VecDeque<(u64, RecoveryOutcome)>> {
    RECOVERY_MEMO.get_or_init(|| Mutex::new(VecDeque::with_capacity(RECOVERY_MEMO_CAPACITY)))
}

fn recovery_key(source_name: &str, source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source_name.hash(&mut hasher);
    source.hash(&mut hasher);
    hasher.finish()
}

fn remembered_recovery(key: u64) -> Option<RecoveryOutcome> {
    let memo = recovery_memo().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    memo.iter().rev().find(|(known, _)| *known == key).map(|(_, outcome)| outcome.clone())
}

fn remember_recovery(key: u64, outcome: RecoveryOutcome) {
    let mut memo = recovery_memo().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    memo.retain(|(known, _)| *known != key);
    if memo.len() >= RECOVERY_MEMO_CAPACITY {
        memo.pop_front();
    }
    memo.push_back((key, outcome));
}

/// How many recoveries of exactly this source ran from scratch (test observation).
#[cfg(test)]
pub(crate) fn recoveries_run_for(source_name: &str, source: &str) -> usize {
    let key = recovery_key(source_name, source);
    RECOVERED_KEYS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter().filter(|known| **known == key).count()
}

fn parsed_from_outcome(source_name: &str, outcome: RecoveryOutcome) -> Result<ParsedProgram> {
    match outcome.repaired_source {
        Some(repaired) => {
            let program = parse_program_strict(source_name, &repaired)?;
            Ok(ParsedProgram { program, diagnostics: vec![outcome.diagnostic], recovered: true })
        }
        None => Err(anyhow!(MietteReportError::new(outcome.diagnostic))),
    }
}

/// Parse, and on a grammar error report it and try to continue with a repaired source.
pub fn parse_program_with_source_name_and_diagnostics(source_name: &str, source: &str) -> Result<ParsedProgram> {
    let parse_error = match BeskidParser::parse(Rule::Program, source) {
        Ok(_) => {
            let program = parse_program_strict(source_name, source)?;
            return Ok(ParsedProgram { program, diagnostics: Vec::new(), recovered: false });
        }
        Err(err) => err,
    };
    let key = recovery_key(source_name, source);
    if let Some(outcome) = remembered_recovery(key) {
        return parsed_from_outcome(source_name, outcome);
    }
    let outcome = recover(source_name, source, &parse_error);
    remember_recovery(key, outcome.clone());
    parsed_from_outcome(source_name, outcome)
}

/// Parse without starting a recovery: the tree of a source that parses, or of one whose
/// recovery is remembered. An editor's outline uses this on every request, so a broken buffer
/// costs it one strict parse, never the candidate parses of a recovery.
pub fn parse_program_without_recovery(source_name: &str, source: &str) -> Option<ParsedProgram> {
    match BeskidParser::parse(Rule::Program, source) {
        Ok(_) => {
            let program = parse_program_strict(source_name, source).ok()?;
            Some(ParsedProgram { program, diagnostics: Vec::new(), recovered: false })
        }
        Err(_) => {
            let outcome = remembered_recovery(recovery_key(source_name, source))?;
            parsed_from_outcome(source_name, outcome).ok()
        }
    }
}

fn recover(source_name: &str, source: &str, parse_error: &pest::error::Error<Rule>) -> RecoveryOutcome {
    #[cfg(test)]
    RECOVERED_KEYS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(recovery_key(source_name, source));
    let diagnosis = diagnose_parse_failure(source_name, source, Rule::Program, parse_error);
    if let Some(repaired) = &diagnosis.repaired_source
        && parse_program_strict_undiagnosed(repaired).is_ok()
    {
        return RecoveryOutcome { repaired_source: Some(repaired.clone()), diagnostic: diagnosis.diagnostic };
    }

    // Candidates are parsed without a diagnosis of their own failure: diagnosing a candidate
    // costs further parses of the file, and only the repair that parses needs to be understood.
    if let Some((candidate_source, repair)) = recover_with_repair_candidates(source, parse_error, |candidate_source| {
        parse_program_strict_undiagnosed(candidate_source).map(|_| candidate_source.to_string())
    }) {
        let repair = match &repair {
            Some(candidate) => candidate.as_engine_repair(),
            None => EngineRepair::Other,
        };
        let diagnosis = refine_with_engine_repair(diagnosis, source, Rule::Program, repair);
        return RecoveryOutcome { repaired_source: Some(candidate_source), diagnostic: diagnosis.diagnostic };
    }

    RecoveryOutcome { repaired_source: None, diagnostic: diagnosis.diagnostic }
}

/// A strict parse whose failure carries no diagnosis (for candidate sources during recovery).
fn parse_program_strict_undiagnosed(source: &str) -> Result<Spanned<Program>, ()> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|_| ())?;
    let pair = pairs.next().ok_or(())?;
    crate::syntax::common::span::with_line_index(source, || Program::parse(pair)).map_err(|_| ())
}

fn parse_program_strict(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = diagnose_parse_failure(source_name, source, Rule::Program, &err).diagnostic;
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| anyhow!(MietteReportError::new(empty_source(source_name, source))))?;
    crate::syntax::common::span::with_line_index(source, || Program::parse(pair)).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}

fn empty_source(source_name: &str, source: &str) -> SemanticDiagnostic {
    let end = if source.is_empty() { 0 } else { 1.min(source.len()) };
    make_diagnostic(
        source_name,
        source,
        crate::syntax::SpanInfo { start: 0, end, line_col_start: (1, 1), line_col_end: (1, 1) },
        "nothing to parse",
        "empty source",
        None,
        Some(codes::EMPTY_SOURCE.to_string()),
        Severity::Error,
    )
}

/// Parse a single expression subtree (used by `code` literal `@{}` holes).
pub fn parse_expression_source(source_name: &str, source: &str) -> Result<Spanned<crate::syntax::Expression>> {
    let trimmed = source.trim();
    let mut pairs = BeskidParser::parse(Rule::Expression, trimmed).map_err(|err| {
        let diagnostic = diagnose_parse_failure(source_name, trimmed, Rule::Expression, &err).diagnostic;
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| anyhow!(MietteReportError::new(empty_source(source_name, source))))?;
    crate::syntax::common::span::with_line_index(trimmed, || {
        crate::syntax::expressions::expression::parse_expression(pair)
    })
    .map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, trimmed, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}
