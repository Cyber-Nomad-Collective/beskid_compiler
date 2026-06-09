#![cfg(feature = "code-widget")]

use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal};
use ratkit::widgets::{CodeState, CodeWidget};

/// Renders a CodeWidget fixture to plain terminal text.
fn render_code_widget(language: &str, source: &str) -> String {
    let mut state = CodeState::default();
    state.source.set_source_string(source);
    let backend = TestBackend::new(72, 8);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            let widget = CodeWidget::from_state(&state)
                .show_line_numbers(true)
                .show_outline(true)
                .language(language);
            frame.render_stateful_widget(widget, frame.area(), &mut state);
        })
        .expect("draw");
    buffer_to_string(terminal.backend().buffer())
}

/// Converts a ratatui test buffer to a newline-separated string.
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

#[test]
fn renders_rust_snapshot() {
    let snapshot = render_code_widget(
        "rust",
        r#"pub struct Counter {
    value: usize,
}

impl Counter {
    pub fn increment(&mut self) {
        self.value += 1;
    }
}"#,
    );
    assert_snapshot!("code_widget_rust", snapshot);
}

#[test]
fn renders_typescript_snapshot() {
    let snapshot = render_code_widget(
        "typescript",
        r#"export class Counter {
  private value = 0;

  increment(): number {
    this.value += 1;
    return this.value;
  }
}"#,
    );
    assert_snapshot!("code_widget_typescript", snapshot);
}

#[test]
fn renders_python_snapshot() {
    let snapshot = render_code_widget(
        "python",
        r#"class Counter:
    def __init__(self):
        self.value = 0

    def increment(self):
        self.value += 1
        return self.value"#,
    );
    assert_snapshot!("code_widget_python", snapshot);
}
