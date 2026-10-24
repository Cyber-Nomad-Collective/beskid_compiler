//! Small deterministic generator (SplitMix64) so runs repeat exactly from a seed without
//! pulling a dependency.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Seed derived from a base seed plus a label (file path, mutation kind, ...), stable across runs.
    pub fn derived(seed: u64, label: &str) -> Self {
        // FNV-1a over the label, mixed with the seed.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in label.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let mut rng = Self(seed ^ hash);
        rng.next_u64();
        rng
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound`; `bound` must be non-zero.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be non-zero");
        (self.next_u64() % bound as u64) as usize
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn derived_depends_on_label() {
        assert_ne!(Rng::derived(1, "a.bd").next_u64(), Rng::derived(1, "b.bd").next_u64());
        assert_eq!(Rng::derived(1, "a.bd").next_u64(), Rng::derived(1, "a.bd").next_u64());
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            assert!(rng.below(5) < 5);
        }
    }
}
