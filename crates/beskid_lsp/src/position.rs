use std::path::Path;

use beskid_analysis::services::SymbolLocation;
use tower_lsp_server::ls_types::{Position, Range};

pub fn offset_in_range(offset: usize, start: usize, end: usize) -> bool {
    let bounded_end = end.max(start.saturating_add(1));
    offset >= start && offset <= bounded_end
}

pub fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut utf16_col = 0u32;

    for (byte_idx, ch) in source.char_indices() {
        if line == position.line && utf16_col >= position.character {
            return byte_idx;
        }

        if ch == '\n' {
            if line == position.line {
                return byte_idx;
            }
            line = line.saturating_add(1);
            utf16_col = 0;
            continue;
        }

        if line == position.line {
            utf16_col = utf16_col.saturating_add(ch.len_utf16() as u32);
        }
    }

    source.len()
}

pub fn offset_range_to_lsp(source: &str, start: usize, end: usize) -> Range {
    let bounded_start = start.min(source.len());
    let bounded_end = end.max(bounded_start).min(source.len());
    Range::new(offset_to_position(source, bounded_start), offset_to_position(source, bounded_end))
}

/// Map a [`SymbolLocation`] to an LSP range, reading `location.path` when it differs from `fallback_path`.
pub fn symbol_location_to_lsp_range(
    location: &SymbolLocation,
    fallback_path: Option<&Path>,
    fallback_source: &str,
) -> Range {
    let same_file = fallback_path.is_some_and(|path| path == location.path.as_path());
    if same_file {
        return offset_range_to_lsp(fallback_source, location.start, location.end);
    }
    if let Ok(text) = std::fs::read_to_string(&location.path) {
        return offset_range_to_lsp(&text, location.start, location.end);
    }
    offset_range_to_lsp(fallback_source, location.start, location.end)
}

pub fn offset_to_position(source: &str, target_offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;

    for (byte_idx, ch) in source.char_indices() {
        if byte_idx >= target_offset {
            return Position::new(line, character);
        }

        if ch == '\n' {
            line = line.saturating_add(1);
            character = 0;
        } else {
            character = character.saturating_add(ch.len_utf16() as u32);
        }
    }

    Position::new(line, character)
}
