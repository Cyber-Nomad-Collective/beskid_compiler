#![cfg(all(feature = "markdown-preview", feature = "code-widget"))]

use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout},
    style::Color,
    widgets::{Block, BorderType, Borders},
    Terminal,
};
use ratkit::widgets::{
    markdown_preview::{
        CacheState, CollapseState, DisplaySettings, DoubleClickState, ExpandableState,
        GitStatsState, MarkdownWidget, ScrollState, SelectionState, SourceState, VimState,
    },
    CodeState, CodeWidget,
};

const WIDTH: u16 = 140;
const HEIGHT: u16 = 18;
const GUTTER_BG: Color = Color::Rgb(10, 14, 20);
const LINE_NUMBER_FG: Color = Color::Rgb(70, 80, 100);
const CURRENT_LINE_BG: Color = Color::Rgb(38, 52, 63);
const TOC_BG: Color = Color::Rgb(30, 32, 38);

/// Verifies both demos keep current-line background out of the gutter.
#[test]
fn current_line_gutter_style_matches_between_demos() {
    let markdown = render_markdown_buffer();
    let code = render_code_buffer();
    let markdown_gutter = first_line_number_cell(&markdown);
    let code_gutter = first_line_number_cell(&code);
    assert_eq!(markdown_gutter.fg, LINE_NUMBER_FG);
    assert_eq!(code_gutter.fg, LINE_NUMBER_FG);
    assert_eq!(markdown_gutter.bg, GUTTER_BG);
    assert_eq!(code_gutter.bg, GUTTER_BG);
}

/// Verifies both demos apply current-line background only to content cells.
#[test]
fn current_line_content_background_matches_between_demos() {
    let markdown = render_markdown_buffer();
    let code = render_code_buffer();
    assert_eq!(first_content_cell(&markdown).bg, CURRENT_LINE_BG);
    assert_eq!(first_content_cell(&code).bg, CURRENT_LINE_BG);
}

/// Verifies both TOC overlays use the same panel background color.
#[test]
fn toc_panel_background_matches_between_demos() {
    let markdown = render_markdown_buffer();
    let code = render_code_buffer();
    assert_eq!(toc_inner_cell(&markdown).bg, TOC_BG);
    assert_eq!(toc_inner_cell(&code).bg, TOC_BG);
}

/// Renders the markdown demo buffer with non-content UI features enabled.
fn render_markdown_buffer() -> Buffer {
    let markdown = "# Title\n\n## API\n\nBody text\n\n## Usage\n\nMore text";
    let mut source = SourceState::default();
    source.set_source_string(markdown.to_string());
    let mut display = DisplaySettings::default();
    display.set_show_document_line_numbers(true);
    let mut widget = MarkdownWidget::new(
        markdown.to_string(),
        ScrollState::default(),
        source,
        CacheState::default(),
        display,
        CollapseState::default(),
        ExpandableState::default(),
        GitStatsState::default(),
        VimState::default(),
        SelectionState::default(),
        DoubleClickState::default(),
    )
    .with_has_pane(true)
    .show_toc(true)
    .show_statusline(true);
    render_buffer(|frame| {
        let chunks = demo_chunks(frame.area());
        frame.render_widget(&mut widget, chunks[1]);
    })
}

/// Renders the code demo buffer with non-content UI features enabled.
fn render_code_buffer() -> Buffer {
    let mut state = CodeState::default();
    state.source.set_source_string(
        "pub struct Counter {\n    value: usize,\n}\n\nimpl Counter {\n    pub fn increment(&mut self) {\n        self.value += 1;\n    }\n}",
    );
    state.display.show_outline = true;
    state.display.show_line_numbers = true;
    state.display.highlight_current_line = true;
    render_buffer(|frame| {
        let chunks = demo_chunks(frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Code");
        let code_area = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);
        let widget = CodeWidget::from_state(&state)
            .show_line_numbers(true)
            .show_outline(true)
            .language("rust");
        frame.render_stateful_widget(widget, code_area, &mut state);
    })
}

/// Splits the demo into dev bar and pane rows.
fn demo_chunks(area: ratatui::layout::Rect) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area)
}

/// Renders a frame and returns the final buffer.
fn render_buffer(draw: impl FnOnce(&mut ratatui::Frame)) -> Buffer {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(draw).expect("draw");
    terminal.backend().buffer().clone()
}

/// Returns the first visible line-number cell.
fn first_line_number_cell(buffer: &Buffer) -> &ratatui::buffer::Cell {
    find_cell(buffer, 2, "1")
}

/// Returns the first visible content cell after the gutter separator.
fn first_content_cell(buffer: &Buffer) -> &ratatui::buffer::Cell {
    let row = 2;
    let mut separators = 0;
    for x in 0..WIDTH {
        if buffer[(x, row)].symbol() == "│" {
            separators += 1;
            if separators == 2 {
                return &buffer[(x + 2, row)];
            }
        }
    }
    panic!("content cell not found")
}

/// Returns a stable inner TOC panel cell.
fn toc_inner_cell(buffer: &Buffer) -> &ratatui::buffer::Cell {
    for y in 2..6 {
        for x in 100..WIDTH {
            let cell = &buffer[(x, y)];
            if cell.bg == TOC_BG {
                return cell;
            }
        }
    }
    panic!("TOC panel cell not found")
}

/// Finds a cell with the requested symbol on one row.
fn find_cell<'a>(buffer: &'a Buffer, row: u16, symbol: &str) -> &'a ratatui::buffer::Cell {
    for x in 0..WIDTH {
        if buffer[(x, row)].symbol() == symbol {
            return &buffer[(x, row)];
        }
    }
    panic!("cell {symbol} not found")
}
