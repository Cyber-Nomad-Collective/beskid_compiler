//! Interactive CodeWidget demo app shell.

use std::time::{Duration, Instant};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use ratkit::{
    prelude::{
        run, CoordinatorAction, CoordinatorApp, CoordinatorEvent, LayoutResult, RunnerConfig,
    },
    widgets::{CodeState, CodeWidget},
};

use crate::{
    input::{handle_keyboard, handle_mouse},
    sample::SAMPLE_RUST_CODE,
};

/// Interactive demo app mirroring the markdown preview demo shell.
pub struct CodeWidgetDemo {
    pub(super) state: CodeState,
    pub(super) code_area: Rect,
    pub(super) mouse_x: u16,
    pub(super) mouse_y: u16,
    pub(super) redraws: u64,
    pub(super) frames_this_second: u32,
    pub(super) fps: u16,
    pub(super) fps_window_start: Instant,
    pub(super) last_move_processed: Instant,
    startup_started_at: Instant,
    startup_probe: bool,
    startup_reported: bool,
    request_quit: bool,
}

impl CodeWidgetDemo {
    /// Creates a demo app with sample Rust code.
    pub fn new(startup_probe: bool, startup_started_at: Instant) -> Self {
        let mut state = CodeState::default();
        state.source.set_source_string(SAMPLE_RUST_CODE);
        state.display.show_outline = true;
        state.display.show_line_numbers = true;
        state.display.highlight_current_line = true;
        Self {
            state,
            code_area: Rect::default(),
            mouse_x: 0,
            mouse_y: 0,
            redraws: 0,
            frames_this_second: 0,
            fps: 0,
            fps_window_start: Instant::now(),
            last_move_processed: Instant::now(),
            startup_started_at,
            startup_probe,
            startup_reported: false,
            request_quit: false,
        }
    }

    /// Updates the displayed frames-per-second counter.
    fn update_fps(&mut self) {
        self.frames_this_second = self.frames_this_second.saturating_add(1);
        let elapsed = self.fps_window_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let elapsed_ms = elapsed.as_millis().max(1) as u32;
            self.fps = ((self.frames_this_second.saturating_mul(1000)) / elapsed_ms) as u16;
            self.frames_this_second = 0;
            self.fps_window_start = Instant::now();
        }
    }

    /// Builds the code widget with demo options.
    pub(super) fn widget(&self) -> CodeWidget {
        CodeWidget::from_state(&self.state)
            .show_line_numbers(true)
            .show_outline(true)
            .language("rust")
    }

    /// Draws the top development metrics bar.
    fn render_dev_bar(&self, frame: &mut Frame, area: Rect) {
        let dev_text = format!(
            " DEV | FPS {:>3} | REDRAWS {:>7} | MOUSE {:>4},{:<4} | q quit | ] TOC | wheel scroll | hover TOC | click TOC jump ",
            self.fps, self.redraws, self.mouse_x, self.mouse_y
        );
        frame.render_widget(
            Paragraph::new(Line::from(dev_text))
                .style(Style::default().fg(Color::Black).bg(Color::Cyan)),
            area,
        );
    }

    /// Emits startup timing once and exits when startup probing is requested.
    fn report_startup_once(&mut self) {
        if self.startup_reported {
            return;
        }
        self.startup_reported = true;
        let ready_ms = self.startup_started_at.elapsed().as_secs_f64() * 1000.0;
        eprintln!("CODE_DEMO_READY_MS={ready_ms:.1}");
        if self.startup_probe {
            self.request_quit = true;
        }
    }
}

impl CoordinatorApp for CodeWidgetDemo {
    /// Handles keyboard, mouse, tick, and resize events.
    fn on_event(&mut self, event: CoordinatorEvent) -> LayoutResult<CoordinatorAction> {
        if self.request_quit {
            return Ok(CoordinatorAction::Quit);
        }
        match event {
            CoordinatorEvent::Keyboard(key) => handle_keyboard(self, key),
            CoordinatorEvent::Mouse(mouse) => handle_mouse(self, mouse),
            CoordinatorEvent::Resize(_) => Ok(CoordinatorAction::Redraw),
            _ => Ok(CoordinatorAction::Continue),
        }
    }

    /// Draws the dev bar, pane, and code widget.
    fn on_draw(&mut self, frame: &mut Frame) {
        self.redraws = self.redraws.saturating_add(1);
        self.update_fps();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Code");
        self.code_area = block.inner(chunks[1]);
        self.render_dev_bar(frame, chunks[0]);
        frame.render_widget(block, chunks[1]);
        frame.render_stateful_widget(self.widget(), self.code_area, &mut self.state);
        self.report_startup_once();
    }
}

/// Runs the interactive CodeWidget sample.
pub fn run_demo() -> std::io::Result<()> {
    let startup_started_at = Instant::now();
    let startup_probe = std::env::args().any(|arg| arg == "--startup-probe");
    let config = RunnerConfig {
        tick_rate: Duration::from_millis(250),
        ..RunnerConfig::default()
    };
    run(
        CodeWidgetDemo::new(startup_probe, startup_started_at),
        config,
    )
}
