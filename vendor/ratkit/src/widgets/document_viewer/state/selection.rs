//! Selection state shared by document viewers.

use ratatui::text::Line;

use crate::widgets::document_viewer::SelectionPos;

/// Tracks rendered-text selection and line-oriented selection.
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    /// Whether visual text selection is active.
    pub active: bool,
    /// Visual selection anchor.
    pub anchor: Option<SelectionPos>,
    /// Visual selection cursor.
    pub cursor: Option<SelectionPos>,
    /// Cached rendered lines for stable copy extraction.
    pub frozen_lines: Option<Vec<Line<'static>>>,
    /// Width used when frozen lines were captured.
    pub frozen_width: usize,
    /// Most recent copied text.
    pub last_copied_text: Option<String>,
}

impl SelectionState {
    /// Creates a new inactive selection state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters visual selection mode at a document-relative position.
    pub fn enter(&mut self, x: i32, y: i32, lines: Vec<Line<'static>>, width: usize) {
        self.active = true;
        self.anchor = Some(SelectionPos::new(x, y));
        self.cursor = Some(SelectionPos::new(x, y));
        self.frozen_lines = Some(lines);
        self.frozen_width = width;
    }

    /// Returns true when visual selection mode is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Exits selection mode and clears active visual bounds.
    pub fn exit(&mut self) {
        self.active = false;
        self.anchor = None;
        self.cursor = None;
        self.frozen_lines = None;
        self.frozen_width = 0;
    }

    /// Returns selected rendered text from frozen lines.
    pub fn get_selected_text(&self) -> Option<String> {
        let (start, end) = self.get_selection()?;
        let lines = self.frozen_lines.as_ref()?;
        Some(extract_text_from_lines(
            lines,
            start.x as usize,
            start.y as usize,
            end.x as usize,
            end.y as usize,
        ))
    }

    /// Returns normalized visual selection bounds.
    pub fn get_selection(&self) -> Option<(SelectionPos, SelectionPos)> {
        let anchor = self.anchor?;
        let cursor = self.cursor?;
        Some(normalize_selection(anchor, cursor))
    }

    /// Returns true when the selection covers at least one cell.
    pub fn has_selection(&self) -> bool {
        let Some((start, end)) = self.get_selection() else {
            return false;
        };
        start != end
    }

    /// Updates the visual selection cursor.
    pub fn update_cursor(&mut self, x: i32, y: i32) {
        self.cursor = Some(SelectionPos::new(x, y));
    }

    /// Selects one zero-based line for line-oriented viewers.
    pub fn select_line(&mut self, line: usize) {
        self.active = true;
        let line = line as i32;
        self.anchor = Some(SelectionPos::new(0, line));
        self.cursor = Some(SelectionPos::new(i32::MAX, line));
    }

    /// Extends line-oriented selection to a zero-based line.
    pub fn extend_to(&mut self, line: usize) {
        if self.anchor.is_none() {
            self.anchor = Some(SelectionPos::new(0, line as i32));
        }
        self.active = true;
        self.cursor = Some(SelectionPos::new(i32::MAX, line as i32));
    }

    /// Clears active visual and line-oriented selection.
    pub fn clear(&mut self) {
        self.active = false;
        self.anchor = None;
        self.cursor = None;
    }

    /// Returns the selected zero-based line range.
    pub fn selected_range(&self) -> Option<std::ops::RangeInclusive<usize>> {
        let (start, end) = self.get_selection()?;
        let start = start.y.max(0) as usize;
        let end = end.y.max(0) as usize;
        Some(start..=end)
    }
}

/// Normalizes two rendered positions into start/end order.
fn normalize_selection(anchor: SelectionPos, cursor: SelectionPos) -> (SelectionPos, SelectionPos) {
    if anchor.y < cursor.y || (anchor.y == cursor.y && anchor.x <= cursor.x) {
        (anchor, cursor)
    } else {
        (cursor, anchor)
    }
}

/// Extracts selected text from rendered lines within visual bounds.
fn extract_text_from_lines(
    lines: &[Line<'static>],
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
) -> String {
    let mut result = String::new();
    for (row_idx, line) in lines.iter().enumerate() {
        if row_idx < start_y || row_idx > end_y {
            continue;
        }
        let line_text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let col_start = if row_idx == start_y { start_x } else { 0 };
        let col_end = if row_idx == end_y {
            end_x.saturating_add(1)
        } else {
            line_text.chars().count()
        };
        let chars: Vec<char> = line_text.chars().collect();
        let actual_start = col_start.min(chars.len());
        let actual_end = col_end.min(chars.len());
        if actual_start < actual_end {
            let selected: String = chars[actual_start..actual_end].iter().collect();
            result.push_str(selected.trim_end());
        }
        if row_idx < end_y {
            result.push('\n');
        }
    }
    result
}
