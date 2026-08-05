//! Shared recovery scoring utilities used to keep candidate ranking consistent.

/// Shared score for sync-semantics operations.
pub(crate) const PRI_SYNC_BOUNDARY_SEMICOLON: u8 = 23;
pub(crate) const PRI_SYNC_DELETE_TOKEN: u8 = 24;
pub(crate) const PRI_SYNC_EXPECTED_TOKEN_BASE: u8 = 34;
pub(crate) const PRI_SYNC_EXPECTED_TOKEN_CONFIDENCE_BAND_WIDTH: u8 = 4;

/// Rank replacement repairs using confidence and token-edit distance.
pub(crate) fn replacement_priority(confidence: u8, source_token: &str, replacement: &str) -> u8 {
    let edit_distance_cost = super::expected_tokens::replacement_text_cost(source_token, replacement).min(8);
    let base = match confidence {
        90..=100 => 26u8,
        80..=89 => 28u8,
        70..=79 => 30u8,
        60..=69 => 32u8,
        _ => 34u8,
    };
    base.saturating_add(edit_distance_cost)
}

/// Rank expected-token insertions with confidence-aware prioritization.
pub(crate) fn expected_token_priority(confidence: u8) -> u8 {
    let confidence = confidence.min(100);
    let band = u16::from(100 - confidence);
    let penalty =
        (band / u16::from(PRI_SYNC_EXPECTED_TOKEN_CONFIDENCE_BAND_WIDTH)).min(u16::from(PRI_SYNC_EXPECTED_TOKEN_BASE));
    PRI_SYNC_EXPECTED_TOKEN_BASE.saturating_sub(u8::try_from(penalty).unwrap_or(u8::MAX))
}
