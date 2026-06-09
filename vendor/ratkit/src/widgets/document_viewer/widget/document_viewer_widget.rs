//! Stateless renderer wrapper for normalized documents.

use ratatui::{buffer::Buffer, layout::Rect};

use crate::widgets::document_viewer::{
    extensions::{render_outline, render_scrollbar, render_statusline},
    foundation::RenderedDocument,
    state::{DisplaySettings, ScrollState, SelectionState},
};

use super::render_document_lines;

/// Renders a normalized document with optional extensions.
#[derive(Debug, Clone)]
pub struct DocumentViewerWidget {
    document: RenderedDocument,
    display: DisplaySettings,
    selection: Option<SelectionState>,
    statusline: Option<String>,
    show_scrollbar: bool,
    outline_hovered: bool,
    outline_hovered_entry: Option<usize>,
}

impl DocumentViewerWidget {
    /// Creates a document viewer for a normalized document.
    pub fn new(document: RenderedDocument, display: DisplaySettings) -> Self {
        Self {
            document,
            display,
            selection: None,
            statusline: None,
            show_scrollbar: false,
            outline_hovered: false,
            outline_hovered_entry: None,
        }
    }

    /// Adds selection overlay state for this render.
    pub fn selection(mut self, selection: SelectionState) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Adds a bottom statusline for this render.
    pub fn statusline(mut self, statusline: impl Into<String>) -> Self {
        self.statusline = Some(statusline.into());
        self
    }

    /// Toggles scrollbar rendering.
    pub fn show_scrollbar(mut self, show_scrollbar: bool) -> Self {
        self.show_scrollbar = show_scrollbar;
        self
    }

    /// Adds outline hover state for TOC overlay rendering.
    pub fn outline_hover(mut self, hovered: bool, entry: Option<usize>) -> Self {
        self.outline_hovered = hovered;
        self.outline_hovered_entry = entry;
        self
    }

    /// Renders the document into a buffer.
    pub fn render(&self, area: Rect, buf: &mut Buffer, scroll: &ScrollState) {
        let (content_area, statusline_area) = split_statusline(area, self.statusline.is_some());
        let outline_area = self.outline_area(content_area);
        let scrollbar_area = self.scrollbar_area(content_area);
        let line_area = if scrollbar_area.is_some() {
            Rect {
                width: content_area.width.saturating_sub(1),
                ..content_area
            }
        } else {
            content_area
        };

        render_document_lines(
            line_area,
            buf,
            &self.document.lines,
            scroll,
            &self.display,
            self.selection.as_ref(),
        );
        if let Some(area) = outline_area {
            render_outline(
                area,
                buf,
                &self.document.outline,
                self.outline_hovered,
                self.outline_hovered_entry,
            );
        }
        if let Some(area) = scrollbar_area {
            render_scrollbar(area, buf, scroll);
        }
        if let (Some(area), Some(statusline)) = (statusline_area, &self.statusline) {
            render_statusline(area, buf, statusline);
        }
    }

    /// Returns the overlay area used for outline/TOC rendering.
    fn outline_area(&self, area: Rect) -> Option<Rect> {
        (self.display.show_outline && !self.document.outline.is_empty()).then_some(area)
    }

    /// Returns the scrollbar area when enabled and useful.
    fn scrollbar_area(&self, area: Rect) -> Option<Rect> {
        (self.show_scrollbar && self.document.lines.len() > area.height as usize).then_some(Rect {
            x: area.x + area.width.saturating_sub(1),
            width: 1,
            ..area
        })
    }
}

/// Splits a bottom statusline area from the main area.
fn split_statusline(area: Rect, has_statusline: bool) -> (Rect, Option<Rect>) {
    if has_statusline && area.height > 1 {
        (
            Rect {
                height: area.height.saturating_sub(1),
                ..area
            },
            Some(Rect {
                y: area.y + area.height.saturating_sub(1),
                height: 1,
                ..area
            }),
        )
    } else {
        (area, None)
    }
}
