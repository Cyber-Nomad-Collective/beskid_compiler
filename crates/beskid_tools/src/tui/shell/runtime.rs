//! Background event loop: ratkit Runner + cross-thread ShellMessage dispatch.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::event;
use crate::tui::kit::{LayoutError, RedrawSignal, RunnerAction, RunnerEvent};

use crate::pipeline::tui::terminal_io::{StderrTerminal, try_init_stderr, try_restore_stderr};
use crate::pipeline::tui::widgets::{init_session_logger, shutdown_session_logger};
use crate::tui::app::{self, BeskidShellApp, map_runner_action, ResizeEvent};
use crate::tui::input::{InputAction, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::effects::{apply_effects, drain_pending_work};
use crate::tui::shell::focus::{FocusTarget, OverlayKind, PaneFocus};
use crate::tui::shell::interrupt::InterruptFlag;
use crate::tui::shell::state::NavTarget;

const TICK: Duration = Duration::from_millis(80);

/// Cross-thread control plane for the interactive session.
pub enum RuntimeOp {
    Update(ShellMessage),
    UpdateAndAck(ShellMessage, Sender<()>),
    SetOverlayVisible {
        kind: OverlayKind,
        visible: bool,
        ack: Option<Sender<()>>,
    },
    Suspend(Sender<()>),
    Resume(Sender<()>),
    WaitFocus {
        target: NavTarget,
        ack: Sender<()>,
    },
    WaitDismiss(Sender<()>),
    Shutdown(Sender<()>),
}

pub struct ShellRuntime {
    tx: Sender<RuntimeOp>,
    join: Option<JoinHandle<io::Result<()>>>,
    interrupt: InterruptFlag,
}

impl ShellRuntime {
    pub fn spawn() -> io::Result<Self> {
        let interrupt = InterruptFlag::new();
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let flag = interrupt.clone();
        let loop_tx = tx.clone();
        let join = thread::Builder::new()
            .name("beskid-tui".into())
            .spawn(move || run_loop(rx, ready_tx, flag, loop_tx))?;
        ready_rx
            .recv()
            .map_err(|_| io::Error::other("tui runtime failed to start"))??;
        Ok(Self {
            tx,
            join: Some(join),
            interrupt,
        })
    }

    pub fn interrupt_flag(&self) -> InterruptFlag {
        self.interrupt.clone()
    }

    pub fn send(&self, op: RuntimeOp) -> io::Result<()> {
        self.tx
            .send(op)
            .map_err(|_| io::Error::other("tui runtime channel closed"))
    }

    pub fn send_update(&self, msg: ShellMessage) -> io::Result<()> {
        self.send(RuntimeOp::Update(msg))
    }

    pub fn send_wait<F>(&self, build: F) -> io::Result<()>
    where
        F: FnOnce(Sender<()>) -> RuntimeOp,
    {
        if self.interrupt.is_set() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "interrupted",
            ));
        }
        let (ack_tx, ack_rx) = mpsc::channel();
        self.send(build(ack_tx))?;
        ack_rx
            .recv()
            .map_err(|_| io::Error::other("tui runtime wait interrupted"))?;
        if self.interrupt.is_set() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "interrupted",
            ));
        }
        Ok(())
    }

    pub fn shutdown(mut self) -> io::Result<()> {
        let (ack_tx, ack_rx) = mpsc::channel();
        let _ = self.tx.send(RuntimeOp::Shutdown(ack_tx));
        if let Some(join) = self.join.take() {
            let _ = ack_rx.recv();
            join.join()
                .map_err(|_| io::Error::other("tui runtime panicked"))??;
        }
        Ok(())
    }
}

fn run_loop(
    rx: Receiver<RuntimeOp>,
    ready: Sender<io::Result<()>>,
    interrupt: InterruptFlag,
    tx: Sender<RuntimeOp>,
) -> io::Result<()> {
    let init_result = (|| -> io::Result<StderrTerminal> {
        let terminal = try_init_stderr()?;
        init_session_logger();
        Ok(terminal)
    })();

    let mut terminal = match init_result {
        Ok(terminal) => {
            let _ = ready.send(Ok(()));
            terminal
        }
        Err(err) => {
            let _ = ready.send(Err(io::Error::other(err.to_string())));
            return Err(err);
        }
    };

    let result = event_loop(&mut terminal, rx, interrupt, tx);
    shutdown_session_logger();
    let _ = terminal.clear();
    try_restore_stderr()?;
    result
}

fn event_loop(
    terminal: &mut StderrTerminal,
    rx: Receiver<RuntimeOp>,
    interrupt: InterruptFlag,
    tx: Sender<RuntimeOp>,
) -> io::Result<()> {
    let redraw_signal = RedrawSignal::new();
    let mut runner = app::new_runner(BeskidShellApp::new(redraw_signal.clone()));
    let size = terminal.size()?;
    runner
        .handle_event(RunnerEvent::Resize(ResizeEvent::new(size.width, size.height)))
        .map_err(runtime_err)?;

    let mut suspended = false;
    let mut pending_focus: Vec<(NavTarget, Sender<()>)> = Vec::new();
    let mut pending_dismiss: Option<Sender<()>> = None;
    let mut last_tick = Instant::now();
    let mut tick_count: u64 = 0;
    let mut dirty = true;
    let mut quitting = false;

    loop {
        if quitting {
            break;
        }

        let mut shutdown_ack = None;
        while let Ok(op) = rx.try_recv() {
            match op {
                RuntimeOp::Update(msg) => {
                    let effects = runner
                        .coordinator_mut()
                        .app_mut()
                        .apply_message(&msg);
                    apply_effects(effects, &tx, &mut runner.coordinator_mut().app_mut().state);
                    redraw_signal.request_redraw();
                    dirty = true;
                }
                RuntimeOp::UpdateAndAck(msg, ack) => {
                    let effects = runner
                        .coordinator_mut()
                        .app_mut()
                        .apply_message(&msg);
                    apply_effects(effects, &tx, &mut runner.coordinator_mut().app_mut().state);
                    redraw_signal.request_redraw();
                    dirty = true;
                    let _ = ack.send(());
                }
                RuntimeOp::SetOverlayVisible { kind, visible, ack } => {
                    {
                        let app = runner.coordinator_mut().app_mut();
                        app.state.set_overlay_visible(kind, visible);
                        if visible {
                            app.state.focus_overlay(kind);
                            match kind {
                                OverlayKind::Pckg if !app.state.pckg.catalog_loaded => {
                                    app.state.pckg.pending_catalog_refresh = true;
                                }
                                OverlayKind::Templates if !app.state.templates.catalog_loaded => {
                                    app.state.templates.pending_catalog_refresh = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    redraw_signal.request_redraw();
                    dirty = true;
                    if let Some(ack) = ack {
                        let _ = ack.send(());
                    }
                }
                RuntimeOp::Suspend(ack) => {
                    if !suspended {
                        try_restore_stderr()?;
                        shutdown_session_logger();
                        suspended = true;
                    }
                    let _ = ack.send(());
                }
                RuntimeOp::Resume(ack) => {
                    if suspended {
                        *terminal = try_init_stderr()?;
                        init_session_logger();
                        let size = terminal.size()?;
                        runner
                            .handle_event(RunnerEvent::Resize(ResizeEvent::new(
                                size.width,
                                size.height,
                            )))
                            .map_err(runtime_err)?;
                        suspended = false;
                        dirty = true;
                    }
                    let _ = ack.send(());
                }
                RuntimeOp::WaitFocus { target, ack } => {
                    runner.coordinator_mut().app_mut().state.awaiting_nav = Some(target);
                    pending_focus.push((target, ack));
                    dirty = true;
                }
                RuntimeOp::WaitDismiss(ack) => {
                    {
                        let app = runner.coordinator_mut().app_mut();
                        app.state.set_overlay_visible(OverlayKind::Summary, true);
                        app.state.focus_overlay(OverlayKind::Summary);
                        app.state.sync_summary_explorer();
                    }
                    pending_dismiss = Some(ack);
                    dirty = true;
                }
                RuntimeOp::Shutdown(ack) => {
                    shutdown_ack = Some(ack);
                    break;
                }
            }
        }

        if let Some(ack) = shutdown_ack {
            let _ = ack.send(());
            break;
        }

        pending_focus.retain(|(target, ack)| {
            if runner.coordinator_mut().app_mut().state.nav_reached(*target) {
                runner.coordinator_mut().app_mut().state.awaiting_nav = None;
                let _ = ack.send(());
                false
            } else {
                true
            }
        });

        if suspended {
            thread::sleep(TICK);
            continue;
        }

        let mut input_action = InputAction::None;
        let now = Instant::now();
        let until_tick = TICK.saturating_sub(now.duration_since(last_tick));

        if event::poll(until_tick)? {
            if let Some(runner_event) = app::runner_event_from_crossterm(event::read()?) {
                let action = runner.handle_event(runner_event).map_err(runtime_err)?;
                if map_runner_action(action) {
                    quitting = true;
                    dirty = true;
                }
                if let Some(result) = runner.coordinator_mut().app_mut().take_input_result() {
                    input_action = apply_input_result(
                        result,
                        &interrupt,
                        &mut runner.coordinator_mut().app_mut().state,
                        &mut pending_focus,
                        &mut pending_dismiss,
                        &mut quitting,
                    );
                }
                if matches!(action, RunnerAction::Redraw) {
                    dirty = true;
                }
            }
        } else if now.duration_since(last_tick) >= TICK {
            tick_count += 1;
            let action = runner
                .handle_event(app::tick_event(tick_count))
                .map_err(runtime_err)?;
            if map_runner_action(action) {
                quitting = true;
            }
            let effects = runner
                .coordinator_mut()
                .app_mut()
                .apply_message(&ShellMessage::Tick);
            apply_effects(
                effects,
                &tx,
                &mut runner.coordinator_mut().app_mut().state,
            );
            last_tick = now;
            dirty = true;
        } else if redraw_signal.take_redraw_request() {
            dirty = true;
        }

        drain_pending_work(&tx, &mut runner.coordinator_mut().app_mut().state);

        if input_action == InputAction::Quit {
            request_quit(
                &interrupt,
                &mut runner.coordinator_mut().app_mut().state,
                &mut pending_focus,
                &mut pending_dismiss,
            );
            quitting = true;
            dirty = true;
        } else {
            resolve_waits(
                &mut pending_focus,
                &mut pending_dismiss,
                &mut runner.coordinator_mut().app_mut().state,
                input_action,
            );
        }

        if dirty && !quitting {
            crate::pipeline::tui::reset_stderr_ansi()?;
            terminal.draw(|frame| {
                let _ = runner.render(frame);
            })?;
            dirty = false;
        }
    }

    Ok(())
}

fn runtime_err(err: LayoutError) -> io::Error {
    io::Error::other(err.to_string())
}

fn apply_input_result(
    result: InputResult,
    interrupt: &InterruptFlag,
    state: &mut crate::tui::shell::state::ShellState,
    pending_focus: &mut Vec<(NavTarget, Sender<()>)>,
    pending_dismiss: &mut Option<Sender<()>>,
    quitting: &mut bool,
) -> InputAction {
    match result {
        InputResult::Quit => {
            request_quit(interrupt, state, pending_focus, pending_dismiss);
            *quitting = true;
            InputAction::Quit
        }
        InputResult::CloseOverlay => {
            if let FocusTarget::Overlay(kind) = state.focus {
                state.set_overlay_visible(kind, false);
                state.focus_base(PaneFocus::Stage);
            }
            skip_pending_navigation(state, pending_focus);
            InputAction::Redraw
        }
        InputResult::SkipNav => {
            skip_pending_navigation(state, pending_focus);
            InputAction::Redraw
        }
        InputResult::Advance => InputAction::Advance,
        InputResult::Handled => InputAction::Redraw,
        InputResult::Bubble => InputAction::None,
    }
}

fn skip_pending_navigation(
    state: &mut crate::tui::shell::state::ShellState,
    pending_focus: &mut Vec<(NavTarget, Sender<()>)>,
) {
    state.awaiting_nav = None;
    for (_, ack) in pending_focus.drain(..) {
        let _ = ack.send(());
    }
}

fn request_quit(
    interrupt: &InterruptFlag,
    state: &mut crate::tui::shell::state::ShellState,
    pending_focus: &mut Vec<(NavTarget, Sender<()>)>,
    pending_dismiss: &mut Option<Sender<()>>,
) {
    interrupt.set();
    state.quit_requested = true;
    state.awaiting_nav = None;
    for (_, ack) in pending_focus.drain(..) {
        let _ = ack.send(());
    }
    if let Some(ack) = pending_dismiss.take() {
        let _ = ack.send(());
    }
}

fn resolve_waits(
    pending_focus: &mut Vec<(NavTarget, Sender<()>)>,
    pending_dismiss: &mut Option<Sender<()>>,
    state: &mut crate::tui::shell::state::ShellState,
    action: InputAction,
) {
    pending_focus.retain(|(target, ack)| {
        if state.nav_reached(*target) {
            state.awaiting_nav = None;
            let _ = ack.send(());
            false
        } else {
            true
        }
    });

    if pending_dismiss.is_some()
        && matches!(action, InputAction::Advance | InputAction::Quit)
        && let Some(ack) = pending_dismiss.take()
    {
        let _ = ack.send(());
    }
}
