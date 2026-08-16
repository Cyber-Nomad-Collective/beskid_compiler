use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_REWRITE};

use crate::syntax::{Program, Spanned};

use super::context::ModInvocationContext;
use super::invoker::{ContractInvoker, RewriteEdit, RewriterOutcome};
use super::types::{AnalyzedContracts, ContractRegistration, ModHostInput, ModHostSession, RewriteResult};

/// Run every Rewriter contract registered by `mod.load`, scheduled by analyzer fixes.
/// Each invocation is dispatched through `invoker`; the returned `RewriteResult`
/// records how many rewrites were applied per contract type so engine and tooling
/// have a per-mod scoreboard for the pipeline.
///
/// When `source` is `Some`, any text edits returned by rewriters are applied
/// right-to-left (see [`apply_edits`]) and the edited source is surfaced through
/// [`RewriteResult::edited_source`]. When `None`, edit application is skipped —
/// callers without source text (e.g. the bare `run_analyze_rewrite` entry point)
/// preserve the previous record-only behavior.
pub(crate) fn run_rewriters(
    program: Spanned<Program>,
    source: Option<&str>,
    session: &ModHostSession,
    analyzed: &AnalyzedContracts,
    input: Option<&ModHostInput<'_>>,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<RewriteResult> {
    observe_phase_result(pipeline, MOD_REWRITE, || {
        let mut registrations: Vec<ContractRegistration> =
            session.registrations().filter(|registration| is_rewrite_registration(registration)).cloned().collect();
        registrations.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| left.type_id.cmp(&right.type_id))
                .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
        });

        let context = input
            .map(|host_input| ModInvocationContext::build(host_input, &[]))
            .unwrap_or_else(ModInvocationContext::empty);
        let mut outcomes: Vec<RewriterOutcome> = Vec::with_capacity(registrations.len());
        for registration in &registrations {
            let outcome = invoker
                .invoke_rewriter(registration, &context.collect_request)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            outcomes.push(outcome);
        }

        // `analyzed` is preserved for future host-side fix dispatch; today the host
        // calls every Rewriter once and expects deterministic completion.
        let _analyzer_count = analyzed.outcomes.len();

        let edited_source = source.and_then(|text| {
            let all_edits: Vec<RewriteEdit> =
                outcomes.iter().flat_map(|outcome| outcome.edits.iter().cloned()).collect();
            if all_edits.is_empty() { None } else { Some(apply_edits(text, &all_edits)) }
        });

        Ok(RewriteResult { program, outcomes, edited_source })
    })
}

/// Apply text edits to `source`, right-to-left, skipping edits that overlap an
/// already-applied edit. Offsets past the source end are clamped. The sort order
/// (start descending, then end descending) guarantees earlier offsets remain valid
/// as later (higher-offset) edits are applied first.
pub(crate) fn apply_edits(source: &str, edits: &[RewriteEdit]) -> String {
    if edits.is_empty() {
        return source.to_string();
    }
    // Normalize all edits to `(start, end, replacement)` triples.
    let mut ranges: Vec<(usize, usize, String)> = edits
        .iter()
        .map(|edit| match edit {
            RewriteEdit::Insert { offset, text } => (*offset, *offset, text.clone()),
            RewriteEdit::Replace { start, end, text } => (*start, *end, text.clone()),
            RewriteEdit::Delete { start, end } => (*start, *end, String::new()),
        })
        .collect();
    // Sort by start descending, then end descending, so we apply right-to-left.
    ranges.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut result = source.to_string();
    let mut last_start = usize::MAX;
    for (start, end, replacement) in &ranges {
        let start = (*start).min(result.len());
        let end = (*end).min(result.len()).max(start);
        // We apply right-to-left (descending start). An edit overlaps an
        // already-applied (rightward) edit when its `end` reaches past that edit's
        // `start`; skip it so the first-applied edit wins.
        if end > last_start {
            continue;
        }
        result.replace_range(start..end, replacement);
        last_start = start;
    }
    result
}

fn is_rewrite_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Rewriter")
}

#[cfg(test)]
mod tests {
    use super::super::invoker::RewriteEdit;
    use super::apply_edits;

    #[test]
    fn apply_edits_inserts_text() {
        let result = apply_edits("hello world", &[RewriteEdit::Insert { offset: 5, text: " big".into() }]);
        assert_eq!(result, "hello big world");
    }

    #[test]
    fn apply_edits_replaces_range() {
        let result =
            apply_edits("unit Main() { return; }", &[RewriteEdit::Replace { start: 0, end: 4, text: "UNIT".into() }]);
        assert_eq!(result, "UNIT Main() { return; }");
    }

    #[test]
    fn apply_edits_deletes_range() {
        let result = apply_edits("abcDEFghi", &[RewriteEdit::Delete { start: 3, end: 6 }]);
        assert_eq!(result, "abcghi");
    }

    #[test]
    fn apply_edits_applies_right_to_left_preserving_offsets() {
        let edits = vec![
            RewriteEdit::Insert { offset: 0, text: ">".into() },
            RewriteEdit::Insert { offset: 11, text: "<".into() },
        ];
        let result = apply_edits("hello world", &edits);
        assert_eq!(result, ">hello world<");
    }

    #[test]
    fn apply_edits_skips_overlapping_edits() {
        // Two overlapping replaces on "ABCDEFGH": [0..4] -> "XX" and [2..6] -> "YY".
        // Sorted descending by start, the rightward edit [2..6] is applied first
        // ("ABYYGH"); the leftward edit [0..4] is skipped because its end (4) reaches
        // into the already-applied region starting at 2.
        let edits = vec![
            RewriteEdit::Replace { start: 0, end: 4, text: "XX".into() },
            RewriteEdit::Replace { start: 2, end: 6, text: "YY".into() },
        ];
        let result = apply_edits("ABCDEFGH", &edits);
        assert_eq!(result, "ABYYGH");
    }

    #[test]
    fn apply_edits_clamps_offsets_past_end() {
        let result = apply_edits("abc", &[RewriteEdit::Replace { start: 2, end: 100, text: "Z".into() }]);
        assert_eq!(result, "abZ");
    }

    #[test]
    fn apply_edits_empty_returns_source() {
        let result = apply_edits("abc", &[]);
        assert_eq!(result, "abc");
    }
}
