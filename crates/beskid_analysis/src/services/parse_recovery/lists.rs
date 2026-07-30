//! Shared list/trailing-separator recovery helpers.

use super::{candidate::RepairCandidate, syntax_primitives};

/// Build trailing-comma list repair candidates for any open/close delimiter pair.
pub(crate) fn trailing_separator_before_close_delimiter<F>(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
    open: u8,
    close: u8,
    inside_list: F,
    placeholder: &'static str,
    delete_priority: u8,
    insert_priority: u8,
    delete_reason: &'static str,
    insert_reason: &'static str,
) where
    F: Fn(&str, usize, usize) -> bool,
{
    let scan_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let Some(open_pos) = syntax_primitives::find_unclosed_delimiter_before(source, scan_pos, open, close) else {
        return;
    };

    if !inside_list(source, open_pos, scan_pos) {
        return;
    }

    let through = syntax_primitives::matching_delimiter_close(source, open_pos, open, close).unwrap_or(source.len());
    let Some(comma_pos) = syntax_primitives::trailing_separator_before_list_close(
        source,
        open_pos,
        through,
        open,
        close,
        b',',
    ) else {
        return;
    };

    candidates.push(RepairCandidate::delete(comma_pos, 1, delete_reason, delete_priority));
    candidates.push(RepairCandidate::insert(insert_at, placeholder, insert_reason, insert_priority));
}

/// Build a trailing-comma "replace-with-close" repair plus insertion fallback for list closers.
///
/// This is useful for cases where the recovery strategy is stronger as a single replace
/// (e.g. `T,` → `T>` in generic/type-angle list tails).
pub(crate) fn replace_trailing_separator_with_close_before_delimiter<F>(
    source: &str,
    error_pos: usize,
    _insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
    open: u8,
    close: u8,
    inside_list: F,
    placeholder: &'static str,
    delete_priority: u8,
    replace_priority: u8,
    insert_priority: u8,
    delete_reason: &'static str,
    replace_reason: &'static str,
    insert_reason: &'static str,
) where
    F: Fn(&str, usize, usize) -> bool,
{
    let scan_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let Some(open_pos) = syntax_primitives::find_unclosed_delimiter_before(source, scan_pos, open, close) else {
        return;
    };

    if !inside_list(source, open_pos, scan_pos) {
        return;
    }

    let through = syntax_primitives::matching_delimiter_close(source, open_pos, open, close).unwrap_or(source.len());
    let Some(comma_pos) = syntax_primitives::trailing_separator_before_list_close(
        source,
        open_pos,
        through,
        open,
        close,
        b',',
    ) else {
        return;
    };

    let close_text = (close as char).to_string();
    let placeholder_pos = comma_pos.saturating_add(1);

    candidates.push(RepairCandidate::delete(comma_pos, 1, delete_reason, delete_priority));
    candidates.push(RepairCandidate::replace(comma_pos, 1, &close_text, replace_reason, replace_priority));
    candidates.push(RepairCandidate::insert(placeholder_pos, placeholder, insert_reason, insert_priority));
}
