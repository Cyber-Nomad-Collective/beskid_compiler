use std::collections::BTreeSet;

pub const BLOCKED_LINK_REASON: &str = "This content contains a link that is not allowed on this registry.";
const MAX_BLOCKED_LINK_PATTERN_LENGTH: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedLinkPattern(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockedLinkPatternError {
    Empty,
    TooLong,
}

impl BlockedLinkPattern {
    pub fn new(pattern: impl AsRef<str>) -> Result<Self, BlockedLinkPatternError> {
        let pattern = pattern.as_ref().trim();
        if pattern.is_empty() {
            return Err(BlockedLinkPatternError::Empty);
        }
        if pattern.len() > MAX_BLOCKED_LINK_PATTERN_LENGTH {
            return Err(BlockedLinkPatternError::TooLong);
        }
        Ok(Self(pattern.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedLinkPatterns(Vec<BlockedLinkPattern>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockedLinkPatternsError {
    Invalid(BlockedLinkPatternError),
    Duplicate,
}

impl BlockedLinkPatterns {
    pub fn from_patterns(
        patterns: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Self, BlockedLinkPatternsError> {
        let mut seen = BTreeSet::new();
        let mut normalized = Vec::new();
        for value in patterns {
            let pattern = BlockedLinkPattern::new(value).map_err(BlockedLinkPatternsError::Invalid)?;
            if !seen.insert(pattern.0.to_ascii_lowercase()) {
                return Err(BlockedLinkPatternsError::Duplicate);
            }
            normalized.push(pattern);
        }
        Ok(Self(normalized))
    }

    pub fn patterns(&self) -> &[BlockedLinkPattern] {
        &self.0
    }

    /// Returns the legacy public reason when a URL-like segment contains a blocked pattern.
    pub fn block_reason(&self, text: impl AsRef<str>) -> Option<&'static str> {
        let text = text.as_ref();
        for segment in url_like_segments(text) {
            if self.0.iter().any(|pattern| contains_ascii_case_insensitive(segment, pattern.as_str())) {
                return Some(BLOCKED_LINK_REASON);
            }
        }
        None
    }
}

fn url_like_segments(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| character.is_whitespace() || matches!(character, '"' | '\'' | '<' | '>' | '(' | ')'))
        .filter(|segment| {
            let lower = segment.to_ascii_lowercase();
            lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
        })
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}
