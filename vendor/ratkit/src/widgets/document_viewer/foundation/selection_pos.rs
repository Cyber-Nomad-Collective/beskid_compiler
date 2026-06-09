//! Visual text-selection coordinates for rendered document viewers.

/// Position in rendered text using document-relative visual coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectionPos {
    /// X coordinate, measured in rendered cells.
    pub x: i32,
    /// Y coordinate, relative to the rendered document start.
    pub y: i32,
}

impl SelectionPos {
    /// Creates a new rendered text position.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}
