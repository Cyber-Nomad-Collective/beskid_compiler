#![cfg(all(feature = "markdown-preview", feature = "code-widget"))]

use insta::assert_snapshot;
use ratatui::{
    backend::TestBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
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

/// Verifies demo chrome snapshots and a feature-level discrepancy report.
#[test]
fn compares_demo_chrome_feature_discrepancies() {
    let markdown = render_markdown_demo_chrome();
    let code = render_code_demo_chrome();
    assert_snapshot!("demo_visual_comparison_markdown", markdown);
    assert_snapshot!("demo_visual_comparison_code", code);
    assert_snapshot!(
        "demo_visual_comparison_feature_report",
        feature_discrepancy_report(&markdown, &code)
    );
}

/// Renders the markdown demo shell with non-highlighting UI features enabled.
fn render_markdown_demo_chrome() -> String {
    let markdown = "# Title\n\n## API\n\nBody text\n\n## Usage\n\nMore text";
    let mut source = SourceState::default();
    source.set_source_string(markdown.to_string());
    let mut scroll = ScrollState::default();
    scroll.update_total_lines(markdown.lines().count().max(1));
    let mut display = DisplaySettings::default();
    display.set_show_document_line_numbers(true);
    let mut widget = MarkdownWidget::new(
        markdown.to_string(),
        scroll,
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
    .show_scrollbar(true)
    .show_statusline(true);

    render_terminal(|frame| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(frame.area());
        frame.render_widget(markdown_dev_bar(), chunks[0]);
        frame.render_widget(&mut widget, chunks[1]);
    })
}

/// Renders the code demo shell with the current CodeWidget demo feature set.
fn render_code_demo_chrome() -> String {
    let mut state = CodeState::default();
    state.source.set_source_string(
        "pub struct Counter {\n    value: usize,\n}\n\nimpl Counter {\n    pub fn increment(&mut self) {\n        self.value += 1;\n    }\n}",
    );
    state.display.show_outline = true;
    state.display.show_line_numbers = true;
    state.display.highlight_current_line = true;

    render_terminal(|frame| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(frame.area());
        frame.render_widget(code_dev_bar(), chunks[0]);
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

/// Builds the markdown demo dev bar text.
fn markdown_dev_bar() -> Paragraph<'static> {
    Paragraph::new(Line::from(
        " DEV | FPS   0 | REDRAWS       1 | MOUSE    0,0    | q quit | ] TOC | wheel scroll | hover TOC | click TOC jump ",
    ))
    .style(Style::default().fg(Color::Black).bg(Color::Cyan))
}

/// Builds the code demo dev bar text.
fn code_dev_bar() -> Paragraph<'static> {
    Paragraph::new(Line::from(
        " DEV | FPS   0 | REDRAWS       1 | MOUSE    0,0    | q quit | ] TOC | wheel scroll | hover TOC | click TOC jump ",
    ))
    .style(Style::default().fg(Color::Black).bg(Color::Cyan))
}

/// Renders a terminal frame into plain text for snapshot comparison.
fn render_terminal(draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(draw).expect("draw");
    buffer_to_string(terminal.backend().buffer())
}

/// Converts a ratatui test buffer into a newline-separated string.
fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            output.push_str(buffer[(x, y)].symbol());
        }
        if y + 1 < buffer.area.height {
            output.push('\n');
        }
    }
    output
}

/// Produces a feature-only discrepancy report, excluding content highlighting.
fn feature_discrepancy_report(markdown: &str, code: &str) -> String {
    [
        feature_line(
            "rounded pane chrome",
            markdown.contains('╭'),
            code.contains('╭'),
        ),
        feature_line(
            "TOC overlay",
            markdown.contains("╭ TOC"),
            code.contains("╭ TOC"),
        ),
        feature_line(
            "TOC toggle hint",
            markdown.contains("] TOC"),
            code.contains("] TOC"),
        ),
        feature_line(
            "TOC hover hint",
            markdown.contains("hover TOC"),
            code.contains("hover TOC"),
        ),
        feature_line(
            "TOC click-jump hint",
            markdown.contains("click TOC jump"),
            code.contains("click TOC jump"),
        ),
        feature_line(
            "document gutter separator",
            markdown.contains("  1 │"),
            code.contains("  1 │"),
        ),
        feature_line(
            "mode statusline",
            markdown.contains("NORMAL"),
            code.contains("NORMAL"),
        ),
        feature_line(
            "percent statusline",
            markdown.contains('%'),
            code.contains('%'),
        ),
        feature_line(
            "symbol outline panel",
            markdown.contains("Outline"),
            code.contains("Outline"),
        ),
    ]
    .join("\n")
}

/// Formats one feature comparison row.
fn feature_line(name: &str, markdown_has: bool, code_has: bool) -> String {
    format!("{name}: markdown={markdown_has} code={code_has}")
}
