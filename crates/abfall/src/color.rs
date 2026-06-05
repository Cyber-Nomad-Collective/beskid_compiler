//! Tri-color marking system for garbage collection
//!
//! Color state and bit assignments are consumed by `GcHeader` in `gc_box.rs`,
//! which packs the tri-color mark into the same `AtomicUsize` as the root count.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Gray = 1,
    Black = 2,
}

impl From<u8> for Color {
    fn from(value: u8) -> Self {
        match value {
            0 => Color::White,
            1 => Color::Gray,
            2 => Color::Black,
            _ => Color::White,
        }
    }
}
