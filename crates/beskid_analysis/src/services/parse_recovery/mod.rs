//! Shared parse-recovery primitives and domain candidate generators.
//!
//! Domains emit [`RepairCandidate`]s; the orchestrator in [`super::parse`] applies
//! them, dedupes repaired sources, and retries a strict parse (capped).

mod candidate;
mod delimiters;
mod expected_tokens;
mod orchestrator;
mod edit;
mod deletions;
mod heuristics;
mod lists;
mod pipeline;
mod policy;
mod engine;
mod expressions;
mod statements;
mod items;
mod scan;
mod ranking;
mod sync;
mod sync_primitives;
mod syntax_primitives;
mod separators;

pub(crate) use orchestrator::collect_repair_candidates;
pub(crate) use pipeline::recover_with_repair_candidates;
