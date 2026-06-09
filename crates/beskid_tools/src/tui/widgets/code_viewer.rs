//! Read-only code viewer wrapping [`ratatui_code_editor::editor::Editor`].
//!
//! Supports syntax highlighting, scrolling, and byte-range marks for diagnostics.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui_code_editor::editor::Editor;
use ratatui_code_editor::theme::vesper;
use ratatui_code_editor::utils::get_lang;

/// Highlighted byte range in the viewer (reusable by future features).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeRegion {
    pub start_byte: usize,
    pub end_byte: usize,
    pub color: &'static str,
}

/// Read-only source panel with optional file path and highlight regions.
pub struct CodeViewerPanel {
    editor: Option<Editor>,
    source_path: Option<PathBuf>,
    fallback_title: String,
    last_area: Rect,
}

impl Default for CodeViewerPanel {
    fn default() -> Self {
        Self {
            editor: None,
            source_path: None,
            fallback_title: "source".into(),
            last_area: Rect::default(),
        }
    }
}

impl CodeViewerPanel {
    pub fn clear(&mut self) {
        self.editor = None;
        self.source_path = None;
        self.fallback_title = "source".into();
    }

    pub fn load_file(&mut self, path: &Path, highlight_line: Option<usize>) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let lang = get_lang(&path.to_string_lossy());
        let mut editor = Editor::new(&lang, &text, vesper())?;
        editor.show_line_numbers(true);
        if let Some(line) = highlight_line.filter(|line| *line > 0) {
            highlight_line_region(&mut editor, line, "255,80,80");
            scroll_to_line(&mut editor, line);
        }
        self.fallback_title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.source_path = Some(path.to_path_buf());
        self.editor = Some(editor);
        Ok(())
    }

    pub fn load_text(&mut self, title: impl Into<String>, text: &str, lang: &str) {
        let title = title.into();
        self.source_path = None;
        self.fallback_title = title;
        self.editor = Editor::new(lang, text, vesper()).ok();
        if let Some(editor) = self.editor.as_mut() {
            editor.show_line_numbers(false);
        }
    }

    pub fn set_regions(&mut self, regions: &[CodeRegion]) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if regions.is_empty() {
            editor.remove_marks();
            return;
        }
        editor.set_marks(
            regions
                .iter()
                .map(|region| (region.start_byte, region.end_byte, region.color))
                .collect(),
        );
    }

    pub fn highlight_line(&mut self, line: usize, color: &'static str) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        highlight_line_region(editor, line, color);
    }

    pub fn scroll_up(&mut self) {
        if let Some(editor) = self.editor.as_mut() {
            editor.scroll_up();
        }
    }

    pub fn scroll_down(&mut self, viewport_height: u16) {
        if let Some(editor) = self.editor.as_mut() {
            editor.scroll_down(viewport_height as usize);
        }
    }

    pub fn title(&self) -> &str {
        self.source_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or(self.fallback_title.as_str())
    }

    pub fn has_content(&self) -> bool {
        self.editor.is_some()
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, title: Option<&str>) {
        self.last_area = area;
        let label = title.unwrap_or_else(|| self.title());
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {label} "));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width < 2 || inner.height < 2 {
            return;
        }
        let Some(editor) = self.editor.as_mut() else {
            Paragraph::new("Select an item to view source or diagnostics.")
                .style(Style::default().fg(Color::DarkGray))
                .render(inner, frame.buffer_mut());
            return;
        };
        editor.focus(&inner);
        frame.render_widget(&*editor, inner);
    }
}

fn highlight_line_region(editor: &mut Editor, line: usize, color: &'static str) {
    let code = editor.code_ref();
    let line_idx = line.saturating_sub(1);
    if line_idx >= code.len_lines() {
        return;
    }
    let start_char = code.line_to_char(line_idx);
    let end_char = if line_idx + 1 < code.len_lines() {
        code.line_to_char(line_idx + 1)
    } else {
        code.len_chars()
    };
    let start_byte = code.char_to_byte(start_char);
    let end_byte = code.char_to_byte(end_char);
    editor.set_marks(vec![(start_byte, end_byte, color)]);
}

fn scroll_to_line(editor: &mut Editor, line: usize) {
    let line_idx = line.saturating_sub(1);
    editor.set_offset_y(line_idx.saturating_sub(2));
    let cursor = editor.code_ref().line_to_char(line_idx.min(editor.code_ref().len_lines().saturating_sub(1)));
    editor.set_cursor(cursor);
    editor.fit_cursor();
}
