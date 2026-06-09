//! Adapters from markdown-specific state into shared document-viewer state.

use crate::widgets::{
    document_viewer,
    markdown_preview::widgets::markdown_widget::state::{
        DisplaySettings as MarkdownDisplaySettings, ScrollState as MarkdownScrollState,
        SourceState as MarkdownSourceState,
    },
};

/// Converts markdown scroll state into shared viewer scroll state.
pub fn markdown_scroll_to_viewer_scroll(
    scroll: &MarkdownScrollState,
    current_visual_line: usize,
    total_lines: usize,
) -> document_viewer::ScrollState {
    let mut viewer_scroll = document_viewer::ScrollState::default();
    viewer_scroll.viewport_height = scroll.viewport_height;
    viewer_scroll.update_total_lines(total_lines.max(1));
    viewer_scroll.set_current_line(current_visual_line.saturating_add(1));
    viewer_scroll.set_offset(scroll.scroll_offset);
    viewer_scroll
}

/// Converts markdown document display choices into shared viewer display settings.
pub fn markdown_display_to_viewer_display(
    display: &MarkdownDisplaySettings,
) -> document_viewer::DisplaySettings {
    let mut viewer_display = display.viewer.clone();
    viewer_display.show_line_numbers = display.show_document_line_numbers();
    viewer_display.highlight_current_line = false;
    viewer_display.show_outline = false;
    viewer_display
}

/// Copies markdown source content into the shared viewer source state.
pub fn markdown_source_to_viewer_source(
    source: &MarkdownSourceState,
) -> document_viewer::SourceState {
    let mut viewer_source = document_viewer::SourceState::default();
    viewer_source.set_source_string(source.content().unwrap_or_default());
    viewer_source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_markdown_scroll_to_zero_based_viewer_scroll() {
        let mut scroll = MarkdownScrollState::default();
        scroll.scroll_offset = 4;
        scroll.current_line = 6;
        scroll.viewport_height = 10;
        scroll.total_lines = 20;
        let viewer_scroll = markdown_scroll_to_viewer_scroll(&scroll, 5, 20);
        assert_eq!(viewer_scroll.offset, 4);
        assert_eq!(viewer_scroll.current_line, 6);
        assert_eq!(viewer_scroll.total_lines, 20);
    }

    #[test]
    fn converts_markdown_source_content_to_viewer_source() {
        let mut source = MarkdownSourceState::default();
        source.set_source_string("# Title".to_string());
        let viewer_source = markdown_source_to_viewer_source(&source);
        assert_eq!(viewer_source.content(), "# Title");
    }
}
