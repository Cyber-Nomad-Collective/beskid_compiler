//! Interactive CodeWidget demo with pane, dev bar, and sample Rust source.
//!
//! Run with:
//! `cargo run --example code_widget_code_widget_demo --features code-widget`

#[path = "code_widget_demo/app.rs"]
mod app;
#[path = "code_widget_demo/input.rs"]
mod input;
#[path = "code_widget_demo/sample.rs"]
mod sample;

/// Runs the interactive CodeWidget sample.
fn main() -> std::io::Result<()> {
    app::run_demo()
}
