//! Shared parse-recovery primitives and domain candidate generators.
//!
//! Domains emit [`RepairCandidate`]s; the orchestrator in [`super::parse`] applies
//! them, dedupes repaired sources, and retries a strict parse (capped).

mod candidate;
mod deletions;
mod delimiters;
mod edit;
mod engine;
mod expected_tokens;
mod expressions;
mod heuristics;
mod items;
mod lists;
mod orchestrator;
mod pipeline;
mod policy;
mod ranking;
mod scan;
mod separators;
mod statements;
mod sync;
mod sync_primitives;
mod syntax_primitives;

pub(crate) use orchestrator::collect_repair_candidates;
pub(crate) use pipeline::recover_with_repair_candidates;
