#![cfg(feature = "markdown-preview")]

use insta::assert_snapshot;
use ratatui::{backend::TestBackend, text::Line, Terminal};
use ratkit::widgets::markdown_preview::{
    markdown_lines_to_document, markdown_lines_to_document_with_source_lines, CacheState,
    CollapseState, DisplaySettings, DoubleClickState, ExpandableState, GitStatsState,
    MarkdownWidget, ScrollState, SelectionState, SourceState, VimState,
};

/// Renders a markdown widget to plain terminal text.
fn render_markdown(markdown: &str) -> String {
    let mut source = SourceState::default();
    source.set_source_string(markdown.to_string());
    let mut scroll = ScrollState::default();
    scroll.update_total_lines(markdown.lines().count().max(1));
    let mut widget = MarkdownWidget::new(
        markdown.to_string(),
        scroll,
        source,
        CacheState::default(),
        DisplaySettings::default(),
        CollapseState::default(),
        ExpandableState::default(),
        GitStatsState::default(),
        VimState::default(),
        SelectionState::default(),
        DoubleClickState::default(),
    )
    .with_has_pane(false)
    .show_toc(false)
    .show_scrollbar(false)
    .show_statusline(false);
    let backend = TestBackend::new(32, 5);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| frame.render_widget(&mut widget, frame.area()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
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

#[test]
fn markdown_adapter_extracts_shared_outline_snapshot() {
    let document = markdown_lines_to_document(Vec::new(), 1, "# Title\n\n## Child");
    let snapshot = document
        .outline
        .iter()
        .map(|item| format!("{}:{}:{}", item.line, item.level, item.title))
        .collect::<Vec<_>>()
        .join("\n");
    assert_snapshot!(snapshot, @r###"
1:0:Title
3:1:Child
"###);
}

#[test]
fn markdown_widget_still_renders_text_snapshot() {
    let snapshot = render_markdown("# Title\n\nBody with `code`");
    assert_snapshot!(snapshot, @r###"
 󰲡 Title                        
                                
Body with code                  
                                
                                "###);
}

#[test]
fn markdown_adapter_preserves_explicit_source_lines() {
    let document = markdown_lines_to_document_with_source_lines(
        vec![Line::from("wrapped one"), Line::from("wrapped two")],
        vec![7, 7],
        "# Title",
    );
    assert_eq!(document.lines[0].source_line, 7);
    assert_eq!(document.lines[1].source_line, 7);
}
