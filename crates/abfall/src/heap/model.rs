use std::time::Duration;

/// GC phase states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GcPhase {
    /// GC is idle, no collection in progress.
    Idle = 0,
    /// GC is marking reachable objects.
    Marking = 1,
    /// GC is sweeping unreachable objects.
    Sweeping = 2,
}

const GC_PHASE_BITS: usize = 2;
const GC_PHASE_MASK: usize = (1 << GC_PHASE_BITS) - 1;

pub(super) fn gc_state(phase: GcPhase, epoch: usize) -> usize {
    (epoch << GC_PHASE_BITS) | phase as usize
}

pub(super) fn state_phase(state: usize) -> GcPhase {
    GcPhase::from((state & GC_PHASE_MASK) as u8)
}

pub(super) fn state_epoch(state: usize) -> usize {
    state >> GC_PHASE_BITS
}

impl From<u8> for GcPhase {
    fn from(value: u8) -> Self {
        match value {
            1 => GcPhase::Marking,
            2 => GcPhase::Sweeping,
            _ => GcPhase::Idle,
        }
    }
}

impl From<GcPhase> for u8 {
    fn from(value: GcPhase) -> Self {
        value as u8
    }
}

/// Point-in-time heap statistics for runtime diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcHeapStats {
    pub bytes_allocated: usize,
    pub object_count: usize,
    pub phase: GcPhase,
    pub collection_threshold: usize,
    pub external_root_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct GcOptions {
    /// Interval between background collection attempts.
    ///
    /// If set to 0, background collection is disabled
    pub collection_interval: Duration,
    /// Work budget for incremental marking steps in background collection
    pub incremental_work_budget: usize,
    /// Work budget for mutator assist (0 = disabled)
    pub assist_work_budget: usize,
    /// Percentage threshold for triggering collection
    ///
    /// This is the percentage of additional memory usage since the last collection
    /// that will trigger a new collection.
    ///
    /// If set to 0 or usize::MAX, threshold-based collection is disabled
    pub threshold_percent: usize,
    /// Percentage threshold for shrinking the collection threshold
    ///
    /// Only applies the new threshold if the calculated threshold shrinks significantly.
    /// (below this percentage)
    ///
    /// 100 means always shrink, 0 means never shrink
    pub threshold_shrink_percent: usize,
    /// Initial & minimum threshold in bytes to trigger collection
    pub min_threshold_bytes: usize,
    /// Maximum allowed heap size in bytes
    pub limit_bytes: usize,
}

impl GcOptions {
    pub const DEFAULT: Self = Self {
        collection_interval: Duration::from_millis(100),
        incremental_work_budget: 100,
        assist_work_budget: 5,
        threshold_percent: 30,
        threshold_shrink_percent: 30,
        min_threshold_bytes: 1024 * 1024,
        limit_bytes: usize::MAX,
    };
    pub const OFF: Self = Self {
        collection_interval: Duration::from_millis(0),
        incremental_work_budget: usize::MAX,
        assist_work_budget: 0,
        threshold_percent: usize::MAX,
        threshold_shrink_percent: 0,
        min_threshold_bytes: usize::MAX,
        limit_bytes: usize::MAX,
    };
    /// Beskid runtime defaults: concurrent/background collection with heap-growth pacing.
    pub const BESKID_DEFAULT: Self = Self {
        collection_interval: Duration::from_millis(100),
        incremental_work_budget: 128,
        assist_work_budget: 8,
        threshold_percent: 100,
        threshold_shrink_percent: 30,
        min_threshold_bytes: 1024 * 1024,
        limit_bytes: usize::MAX,
    };
    /// Faster collection profile for deterministic tests.
    pub const BESKID_TEST: Self = Self {
        collection_interval: Duration::from_millis(10),
        incremental_work_budget: 512,
        assist_work_budget: 16,
        threshold_percent: 30,
        threshold_shrink_percent: 100,
        min_threshold_bytes: 64 * 1024,
        limit_bytes: usize::MAX,
    };

    #[inline]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    #[inline]
    pub const fn off() -> Self {
        Self::OFF
    }

    #[inline]
    pub const fn beskid_default() -> Self {
        Self::BESKID_DEFAULT
    }

    #[inline]
    pub const fn beskid_test() -> Self {
        Self::BESKID_TEST
    }

    #[inline]
    pub(super) fn is_threshold_off(&self) -> bool {
        self.threshold_percent == 0 || self.threshold_percent == !0
    }

    #[inline]
    pub(super) fn is_limit_off(&self) -> bool {
        self.limit_bytes == usize::MAX
    }

    #[inline]
    pub(super) fn is_background_collection_off(&self) -> bool {
        self.is_threshold_off() || self.collection_interval.as_millis() == 0
    }

    #[inline]
    pub(super) fn is_completely_off(&self) -> bool {
        self.is_threshold_off() && self.is_limit_off()
    }

    /// Compute the next heap-growth pacing threshold.
    pub(super) fn calculate_threshold(&self, old_threshold: usize, live_usage: usize) -> usize {
        if self.is_threshold_off() {
            usize::MAX
        } else {
            let new_threshold = live_usage + (live_usage * self.threshold_percent) / 100;
            if new_threshold < old_threshold {
                if self.threshold_shrink_percent == 0 {
                    return old_threshold;
                } else if self.threshold_shrink_percent < 100 {
                    let shrink_limit = (old_threshold * self.threshold_shrink_percent) / 100;
                    if new_threshold > shrink_limit {
                        return old_threshold;
                    }
                }
            }
            if new_threshold < self.min_threshold_bytes { self.min_threshold_bytes } else { new_threshold }
        }
    }
}

impl Default for GcOptions {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}
