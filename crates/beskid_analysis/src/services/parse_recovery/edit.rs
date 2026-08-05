//! Text edit application for recovery candidates.

use super::candidate::RepairCandidate;

/// Apply a single repair to `source`. Returns `None` if the edit is out of range.
pub(crate) fn apply_repair(source: &str, candidate: &RepairCandidate) -> Option<String> {
    let pos = candidate.position;
    if pos > source.len() {
        return None;
    }
    match &candidate.kind {
        super::candidate::RepairKind::InsertStatic { text } => {
            let mut repaired = String::with_capacity(source.len() + text.len());
            let safe_pos = if source.is_char_boundary(pos) { pos } else { source.len() };
            repaired.push_str(&source[..safe_pos]);
            repaired.push_str(text);
            repaired.push_str(&source[safe_pos..]);
            Some(repaired)
        }
        super::candidate::RepairKind::InsertDynamic { text } => {
            let mut repaired = String::with_capacity(source.len() + text.len());
            let safe_pos = if source.is_char_boundary(pos) { pos } else { source.len() };
            repaired.push_str(&source[..safe_pos]);
            repaired.push_str(text);
            repaired.push_str(&source[safe_pos..]);
            Some(repaired)
        }
        super::candidate::RepairKind::Delete { len } => {
            let end = pos.saturating_add(*len);
            if end > source.len() {
                return None;
            }
            let mut repaired = String::with_capacity(source.len().saturating_sub(*len));
            repaired.push_str(&source[..pos]);
            let safe_end = if source.is_char_boundary(end) { end } else { source.len() };
            repaired.push_str(&source[safe_end..]);
            Some(repaired)
        }
        super::candidate::RepairKind::Replace { len, text } => {
            let end = pos.saturating_add(*len);
            if end > source.len() {
                return None;
            }
            let mut repaired = String::with_capacity(source.len().saturating_sub(*len) + text.len());
            let safe_pos = if source.is_char_boundary(pos) { pos } else { source.len() };
            let safe_end = if source.is_char_boundary(end) { end } else { source.len() };
            repaired.push_str(&source[..safe_pos]);
            repaired.push_str(text);
            repaired.push_str(&source[safe_end..]);
            Some(repaired)
        }
    }
}
