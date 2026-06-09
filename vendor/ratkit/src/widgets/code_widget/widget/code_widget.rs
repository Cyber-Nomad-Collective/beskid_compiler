//! Stateful ratatui widget for read-only source code viewing.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};

use crate::widgets::{
    code_widget::{
        foundation::CodeEvent,
        parser::{detect_language, extract_symbol_outline, highlight_code_lines},
        state::CodeState,
    },
    document_viewer::{
        handle_viewer_key, handle_viewer_mouse, DocumentViewerWidget, RenderedDocument,
    },
};

use super::{
    handle_outline_click::handle_outline_click, handle_outline_hover::handle_outline_hover,
    viewer_content_area::viewer_content_area,
};

/// Read-only source code viewer with syntax highlighting and symbol outline.
#[derive(Debug, Clone, Default)]
pub struct CodeWidget {
    show_line_numbers: Option<bool>,
    relative_line_numbers: Option<bool>,
    show_outline: Option<bool>,
    language_override: Option<String>,
}

impl CodeWidget {
    /// Creates a widget configured from current state defaults.
    pub fn from_state(_state: &CodeState) -> Self {
        Self::default()
    }

    /// Sets line-number visibility for this render.
    pub fn show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = Some(show);
        self
    }

    /// Sets relative line-number visibility for this render.
    pub fn relative_line_numbers(mut self, show: bool) -> Self {
        self.relative_line_numbers = Some(show);
        self
    }

    /// Sets outline visibility for this render.
    pub fn show_outline(mut self, show: bool) -> Self {
        self.show_outline = Some(show);
        self
    }

    /// Overrides language detection for this render.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language_override = Some(language.into());
        self
    }

    /// Applies keyboard input to code-widget state.
    pub fn handle_key(&self, key: KeyEvent, state: &mut CodeState) -> CodeEvent {
        if matches!(key.code, KeyCode::Char('y')) {
            return state
                .copy_selection()
                .map(|text| CodeEvent::Copied { text })
                .unwrap_or(CodeEvent::None);
        }

        let before = state.scroll.current_line;
        if handle_viewer_key(key, &mut state.scroll, &mut state.vim) {
            if before != state.scroll.current_line {
                state.selection.clear();
                CodeEvent::Navigated {
                    line: state.scroll.current_line,
                }
            } else {
                CodeEvent::None
            }
        } else {
            CodeEvent::None
        }
    }

    /// Applies mouse input to code-widget state.
    pub fn handle_mouse(&self, event: MouseEvent, area: Rect, state: &mut CodeState) -> CodeEvent {
        if let Some(event) = self.handle_outline_mouse(event, area, state) {
            return event;
        }
        if handle_viewer_mouse(event, area, &mut state.scroll, &mut state.selection) {
            CodeEvent::SelectionChanged
        } else {
            CodeEvent::None
        }
    }

    /// Applies mouse input to the outline TOC overlay.
    fn handle_outline_mouse(
        &self,
        event: MouseEvent,
        area: Rect,
        state: &mut CodeState,
    ) -> Option<CodeEvent> {
        if !self.display_for_render(state).show_outline || state.outline.is_empty() {
            return None;
        }
        let content_area = viewer_content_area(area);
        match event.kind {
            MouseEventKind::Moved => Some(handle_outline_hover(event, content_area, state)),
            MouseEventKind::Down(MouseButton::Left) => {
                handle_outline_click(event, content_area, state)
            }
            _ => None,
        }
    }

    /// Builds the normalized document for the current state.
    fn build_document(&self, state: &mut CodeState) -> RenderedDocument {
        let override_language = self
            .language_override
            .as_deref()
            .or(state.language_override());
        let language = detect_language(
            state.source.source_path(),
            state.source.content(),
            override_language,
        );
        let display = self.display_for_render(state);
        let content = state.source.content().to_string();
        state.remember_render_cache(&content, &display, &language);
        let lines = highlight_code_lines(&content, &language);
        let outline = extract_symbol_outline(&content, &language)
            .into_iter()
            .map(|item| item.into_document_item())
            .collect::<Vec<_>>();
        state.outline = outline.clone();
        state.scroll.update_total_lines(lines.len());
        RenderedDocument::new(lines, outline)
    }

    /// Applies render-specific display overrides.
    fn display_for_render(
        &self,
        state: &CodeState,
    ) -> crate::widgets::document_viewer::DisplaySettings {
        let mut display = state.display.clone();
        if let Some(show) = self.show_line_numbers {
            display.show_line_numbers = show;
        }
        if let Some(show) = self.relative_line_numbers {
            display.relative_line_numbers = show;
        }
        if let Some(show) = self.show_outline {
            display.show_outline = show;
        }
        display
    }
}

impl StatefulWidget for CodeWidget {
    type State = CodeState;

    /// Renders the code widget into the target buffer.
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let document = self.build_document(state);
        state.scroll.ensure_current_visible(area.height as usize);
        let viewer = DocumentViewerWidget::new(document, self.display_for_render(state))
            .selection(state.selection.clone())
            .show_scrollbar(true)
            .statusline(format!(
                "{}%/{}",
                (state.scroll.current_line * 100) / state.scroll.total_lines.max(1),
                state.scroll.total_lines
            ))
            .outline_hover(state.outline_hovered, state.outline_hovered_entry);
        viewer.render(area, buf, &state.scroll);
    }
}
