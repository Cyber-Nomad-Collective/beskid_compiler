//! Unified state for the code widget.

use std::hash::{Hash, Hasher};

use crate::widgets::{
    code_widget::foundation::CodeLanguage,
    document_viewer::{
        CacheState, DisplaySettings, DocumentOutlineItem, ScrollState, SelectionState, SourceState,
        VimState,
    },
};

/// Bundles source, viewport, selection, and outline state for `CodeWidget`.
#[derive(Debug, Clone, Default)]
pub struct CodeState {
    /// Source content and optional file path.
    pub source: SourceState,
    /// Viewport scroll state.
    pub scroll: ScrollState,
    /// Line-oriented selection state.
    pub selection: SelectionState,
    /// Vim navigation prefix state.
    pub vim: VimState,
    /// Display settings.
    pub display: DisplaySettings,
    /// Lightweight render cache marker.
    pub cache: CacheState,
    /// Last extracted outline.
    pub outline: Vec<DocumentOutlineItem>,
    /// Whether the outline TOC overlay is hovered.
    pub outline_hovered: bool,
    /// Currently hovered outline TOC entry.
    pub outline_hovered_entry: Option<usize>,
    /// Optional state-level language override.
    pub language_override: Option<String>,
}

impl CodeState {
    /// Sets a language override on the state.
    pub fn set_language_override(&mut self, language: impl Into<String>) {
        self.language_override = Some(language.into());
    }

    /// Returns selected source lines joined with newlines.
    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection.selected_range()?;
        let lines: Vec<&str> = self.source.content().lines().collect();
        let selected: Vec<&str> = range
            .filter_map(|index| lines.get(index).copied())
            .collect();
        if selected.is_empty() {
            None
        } else {
            Some(selected.join("\n"))
        }
    }

    /// Stores the selected text as the latest copied text.
    pub fn copy_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.selection.last_copied_text = Some(text.clone());
        Some(text)
    }

    /// Returns the effective state-level language override.
    pub fn language_override(&self) -> Option<&str> {
        self.language_override.as_deref()
    }

    /// Stores a render cache hash for content, display settings, and language.
    pub fn remember_render_cache(
        &mut self,
        content: &str,
        display: &DisplaySettings,
        language: &CodeLanguage,
    ) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        language.hash(&mut hasher);
        display.show_line_numbers.hash(&mut hasher);
        display.relative_line_numbers.hash(&mut hasher);
        display.highlight_current_line.hash(&mut hasher);
        display.show_outline.hash(&mut hasher);
        display.tab_width.hash(&mut hasher);
        self.cache.set_content_hash(hasher.finish());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hash_changes_with_content() {
        let mut state = CodeState::default();
        let display = DisplaySettings::default();
        state.remember_render_cache("one", &display, &CodeLanguage::Rust);
        let first = state.cache.content_hash;
        state.remember_render_cache("two", &display, &CodeLanguage::Rust);
        assert_ne!(first, state.cache.content_hash);
    }

    #[test]
    fn cache_hash_changes_with_display() {
        let mut state = CodeState::default();
        let mut display = DisplaySettings::default();
        state.remember_render_cache("one", &display, &CodeLanguage::Rust);
        let first = state.cache.content_hash;
        display.show_outline = !display.show_outline;
        state.remember_render_cache("one", &display, &CodeLanguage::Rust);
        assert_ne!(first, state.cache.content_hash);
    }
}
