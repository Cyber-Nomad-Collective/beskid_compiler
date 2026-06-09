//! Salsa query hit/miss counters surfaced through [`beskid_pipeline`].

use std::sync::atomic::{AtomicU64, Ordering};

use beskid_pipeline::{PipelineObserver, emit_work_unit, phases};

/// Tracing target for Salsa query execution and invalidation.
pub const SALSA_TRACE_TARGET: &str = "beskid.queries.salsa";

static QUERY_HITS: AtomicU64 = AtomicU64::new(0);
static QUERY_MISSES: AtomicU64 = AtomicU64::new(0);
static REVISION_BUMPS: AtomicU64 = AtomicU64::new(0);

/// Record a query outcome with a structured span (`query`, `outcome`, optional `invalidation_reason`).
pub fn trace_query(name: &'static str, hit: bool) {
    trace_query_with_reason(name, hit, None);
}

/// Record a query outcome with an explicit invalidation reason when applicable.
pub fn trace_query_with_reason(name: &'static str, hit: bool, invalidation_reason: Option<&str>) {
    let outcome = if hit { "hit" } else { "miss" };
    let span = tracing::debug_span!(
        target: SALSA_TRACE_TARGET,
        "query",
        query = name,
        outcome,
        invalidation_reason = invalidation_reason.unwrap_or_default(),
    );
    let _guard = span.enter();
    if hit {
        QUERY_HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        QUERY_MISSES.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn record_query_hit() {
    QUERY_HITS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_query_miss() {
    QUERY_MISSES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_revision_bump() {
    REVISION_BUMPS.fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> (u64, u64, u64) {
    (
        QUERY_HITS.load(Ordering::Relaxed),
        QUERY_MISSES.load(Ordering::Relaxed),
        REVISION_BUMPS.load(Ordering::Relaxed),
    )
}

pub fn reset() {
    QUERY_HITS.store(0, Ordering::Relaxed);
    QUERY_MISSES.store(0, Ordering::Relaxed);
    REVISION_BUMPS.store(0, Ordering::Relaxed);
}

/// Emit current Salsa counters to a pipeline observer.
pub fn emit_salsa_stats<O: PipelineObserver + ?Sized>(obs: Option<&O>) {
    let (hits, misses, bumps) = snapshot();
    let disk = beskid_analysis::projects::assembly::disk_cache_stats();
    emit_work_unit(
        obs,
        phases::SALSA_QUERY_HIT,
        hits,
        hits.saturating_add(misses).max(1),
        "Salsa query hits",
    );
    emit_work_unit(
        obs,
        phases::SALSA_QUERY_MISS,
        misses,
        hits.saturating_add(misses).max(1),
        "Salsa query misses",
    );
    emit_work_unit(
        obs,
        phases::SALSA_REVISION_BUMP,
        bumps,
        bumps.max(1),
        "Salsa revision bumps",
    );
    emit_work_unit(
        obs,
        phases::SALSA_ARTIFACT_DISK_HIT,
        disk.hits as u64,
        (disk.hits + disk.misses).max(1) as u64,
        "Salsa artifact disk hits",
    );
    emit_work_unit(
        obs,
        phases::SALSA_ARTIFACT_DISK_MISS,
        disk.misses as u64,
        (disk.hits + disk.misses).max(1) as u64,
        "Salsa artifact disk misses",
    );
}
