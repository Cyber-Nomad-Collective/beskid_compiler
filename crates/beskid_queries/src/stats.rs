//! Salsa query hit/miss counters surfaced through [`beskid_pipeline`].

use std::sync::atomic::{AtomicU64, Ordering};

use beskid_pipeline::{PipelineObserver, emit_work_unit, phases};

static QUERY_HITS: AtomicU64 = AtomicU64::new(0);
static QUERY_MISSES: AtomicU64 = AtomicU64::new(0);
static REVISION_BUMPS: AtomicU64 = AtomicU64::new(0);

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
