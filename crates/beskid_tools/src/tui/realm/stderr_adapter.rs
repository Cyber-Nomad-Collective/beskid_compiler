//! Stderr [`TerminalAdapter`] — pipeline and `beskid hi` render on stderr.

use std::io::{self, Stderr, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{execute, queue};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::{CompletedFrame, Frame, TerminalOptions};
use tuirealm::terminal::{TerminalAdapter, TerminalError, TerminalResult};

const MODE_RAW: u8 = 0b0000_0001;
const MODE_ALTERNATE: u8 = 0b0000_0010;
const MODE_MOUSE: u8 = 0b0000_0100;

/// Crossterm terminal adapter backed by stderr (stdout stays free for tool output).
#[derive(Debug)]
pub struct StderrTerminalAdapter {
    terminal: Terminal<CrosstermBackend<Stderr>>,
    modes: Arc<AtomicU8>,
}

impl StderrTerminalAdapter {
    pub fn new() -> TerminalResult<Self> {
        Self::new_with_options(TerminalOptions::default())
    }

    pub fn new_with_options(options: TerminalOptions) -> TerminalResult<Self> {
        let backend = CrosstermBackend::new(io::stderr());
        let terminal = Terminal::with_options(backend, options)
            .map_err(|_| TerminalError::Other("cannot connect stderr terminal"))?;
        let modes = Arc::new(AtomicU8::new(0));
        Self::panic_handler(modes.clone());
        Ok(Self { terminal, modes })
    }

    pub fn restore(&mut self) -> std::io::Result<()> {
        let writer = self.terminal.backend_mut();
        let modes = self.modes.swap(0, Ordering::AcqRel);
        if modes & MODE_MOUSE != 0 {
            queue!(writer, DisableMouseCapture)?;
        }
        if modes & MODE_ALTERNATE != 0 {
            queue!(writer, LeaveAlternateScreen)?;
        }
        if modes & MODE_RAW != 0 {
            disable_raw_mode()?;
        }
        writer.flush()
    }

    fn panic_handler(modes: Arc<AtomicU8>) {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let mut stderr = io::stderr();
            let modes = modes.swap(0, Ordering::AcqRel);
            if modes & MODE_MOUSE != 0 {
                let _ = queue!(stderr, DisableMouseCapture);
            }
            if modes & MODE_ALTERNATE != 0 {
                let _ = queue!(stderr, LeaveAlternateScreen);
            }
            if modes & MODE_RAW != 0 {
                let _ = disable_raw_mode();
            }
            let _ = stderr.flush();
            hook(info);
        }));
    }

    fn set_mode(&self, bit: u8) {
        let active = self.modes.load(Ordering::SeqCst);
        self.modes.store(active | bit, Ordering::SeqCst);
    }

    fn unset_mode(&self, bit: u8) {
        let active = self.modes.load(Ordering::SeqCst);
        self.modes.store(active & !bit, Ordering::SeqCst);
    }
}

impl TerminalAdapter for StderrTerminalAdapter {
    type Backend = CrosstermBackend<Stderr>;

    fn clear_screen(&mut self) -> TerminalResult<()> {
        self.terminal
            .clear()
            .map_err(|_| TerminalError::CannotClear)
    }

    fn enable_raw_mode(&mut self) -> TerminalResult<()> {
        enable_raw_mode()
            .map_err(|_| TerminalError::CannotToggleRawMode)
            .inspect(|_| self.set_mode(MODE_RAW))
    }

    fn disable_raw_mode(&mut self) -> TerminalResult<()> {
        disable_raw_mode()
            .map_err(|_| TerminalError::CannotToggleRawMode)
            .inspect(|_| self.unset_mode(MODE_RAW))
    }

    fn enter_alternate_screen(&mut self) -> TerminalResult<()> {
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)
            .map_err(|_| TerminalError::CannotEnterAlternateMode)
            .inspect(|_| self.set_mode(MODE_ALTERNATE))
    }

    fn leave_alternate_screen(&mut self) -> TerminalResult<()> {
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|_| TerminalError::CannotLeaveAlternateMode)
        .inspect(|_| self.unset_mode(MODE_ALTERNATE))
    }

    fn enable_mouse_capture(&mut self) -> TerminalResult<()> {
        execute!(self.raw_mut().backend_mut(), EnableMouseCapture)
            .map_err(|_| TerminalError::CannotToggleMouseCapture)
            .inspect(|_| self.set_mode(MODE_MOUSE))
    }

    fn disable_mouse_capture(&mut self) -> TerminalResult<()> {
        execute!(self.raw_mut().backend_mut(), DisableMouseCapture)
            .map_err(|_| TerminalError::CannotToggleMouseCapture)
            .inspect(|_| self.unset_mode(MODE_MOUSE))
    }

    fn raw_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stderr>> {
        &mut self.terminal
    }

    fn raw(&self) -> &Terminal<CrosstermBackend<Stderr>> {
        &self.terminal
    }
}

impl Drop for StderrTerminalAdapter {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

impl StderrTerminalAdapter {
    pub fn draw<F>(&mut self, render_callback: F) -> TerminalResult<CompletedFrame<'_>>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.raw_mut()
            .draw(render_callback)
            .map_err(|_| TerminalError::CannotDrawFrame)
    }

    /// Release the alternate screen and raw mode so a child process can use the terminal.
    pub fn suspend_for_subprocess(&mut self) -> TerminalResult<()> {
        self.disable_mouse_capture()?;
        self.leave_alternate_screen()?;
        self.disable_raw_mode()?;
        Ok(())
    }

    /// Re-enter TUI modes after a child process finishes.
    pub fn resume_after_subprocess(&mut self) -> TerminalResult<()> {
        self.enable_raw_mode()?;
        self.enter_alternate_screen()?;
        self.enable_mouse_capture()?;
        Ok(())
    }
}
