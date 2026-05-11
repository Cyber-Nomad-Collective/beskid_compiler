//! Minimal shapes mirroring `beskid_analysis` patterns; only used as a text fixture for syn.

#[beskid_reflect]
pub enum SampleNode {
    Alpha,
    Beta(u32),
    Gamma { x: i32 },
}

#[beskid_reflect]
pub struct SampleRecord {
    a: u8,
}
