//! Background event loop: tuirealm Application + cross-thread ShellMessage dispatch.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tuirealm::application::{Application, PollStrategy};
use tuirealm::event::NoUserEvent;
use tuirealm::listener::EventListenerCfg;
use tuirealm::terminal::TerminalAdapter;

use crate::pipeline::tui::widgets::{init_session_logger, shutdown_session_logger};
use crate::tui::app::BeskidShellApp;
use crate::tui::input::{InputAction, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::realm::shell_event::ShellRealmEvent;
use crate::tui::realm::{
    PipelineShellComponent, PipelineShellId, PipelineShellMsg, StderrTerminalAdapter,
};
use crate::tui::shell::effects::{apply_effects, drain_pending_work};
use crate::tui::shell::focus::{FocusTarget, OverlayKind, PaneFocus};
use crate::tui::shell::interrupt::InterruptFlag;
use crate::tui::shell::state::NavTarget;
use crate::tui::signals::RedrawSignal;

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
            return Err(io::Error::new(io::ErrorKind::Interrupted, "interrupted"));
        }
        let (ack_tx, ack_rx) = mpsc::channel();
        self.send(build(ack_tx))?;
        ack_rx
            .recv()
            .map_err(|_| io::Error::other("tui runtime wait interrupted"))?;
        if self.interrupt.is_set() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "interrupted"));
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
    let init_result: io::Result<()> = {
        init_session_logger();
        Ok(())
    };

    if let Err(err) = init_result {
        let _ = ready.send(Err(io::Error::other(err.to_string())));
        return Err(err);
    }

    let listener = EventListenerCfg::default()
        .crossterm_input_listener(Duration::from_millis(10), 3)
        .tick_interval(TICK);

    let mut application = Application::init(listener);
    let redraw_signal = RedrawSignal::new();
    let shell_component = PipelineShellComponent::new(BeskidShellApp::new(redraw_signal.clone()));
    application
        .mount(PipelineShellId::Root, Box::new(shell_component), Vec::new())
        .map_err(runtime_err)?;
    application
        .active(&PipelineShellId::Root)
        .map_err(runtime_err)?;

    let mut terminal = StderrTerminalAdapter::new().map_err(runtime_err)?;
    terminal.enable_raw_mode().map_err(runtime_err)?;
    terminal.enter_alternate_screen().map_err(runtime_err)?;
    terminal.enable_mouse_capture().map_err(runtime_err)?;

    let size = terminal.raw().size().map_err(io::Error::other)?;
    shell_mut(&mut application).handle_shell_event(ShellRealmEvent::Resize {
        width: size.width,
        height: size.height,
    });

    let _ = ready.send(Ok(()));

    let result = event_loop(&mut application, &mut terminal, rx, interrupt, tx);
    shutdown_session_logger();
    let _ = terminal.clear_screen();
    terminal.restore().map_err(io::Error::other)?;
    result
}

fn shell_mut(
    application: &mut Application<PipelineShellId, PipelineShellMsg, NoUserEvent>,
) -> &mut PipelineShellComponent {
    let component = application
        .get_component_mut(&PipelineShellId::Root)
        .expect("pipeline shell mounted");
    PipelineShellComponent::as_any_mut(component)
}

fn event_loop(
    application: &mut Application<PipelineShellId, PipelineShellMsg, NoUserEvent>,
    terminal: &mut StderrTerminalAdapter,
    rx: Receiver<RuntimeOp>,
    interrupt: InterruptFlag,
    tx: Sender<RuntimeOp>,
) -> io::Result<()> {
    let mut suspended = false;
    let mut pending_focus: Vec<(NavTarget, Sender<()>)> = Vec::new();
    let mut pending_dismiss: Option<Sender<()>> = None;
    let mut dirty = true;
    let mut quitting = false;
    let mut last_tick = Instant::now();

    loop {
        if quitting {
            break;
        }

        let mut shutdown_ack = None;
        while let Ok(op) = rx.try_recv() {
            match op {
                RuntimeOp::Update(msg) => {
                    apply_runtime_message(application, &msg, &tx);
                    redraw_signal_request(application);
                    dirty = true;
                }
                RuntimeOp::UpdateAndAck(msg, ack) => {
                    apply_runtime_message(application, &msg, &tx);
                    redraw_signal_request(application);
                    dirty = true;
                    let _ = ack.send(());
                }
                RuntimeOp::SetOverlayVisible { kind, visible, ack } => {
                    {
                        let shell = shell_mut(application);
                        shell.app.state.set_overlay_visible(kind, visible);
                        if visible {
                            shell.app.state.focus_overlay(kind);
                            match kind {
                                OverlayKind::Pckg if !shell.app.state.pckg.catalog_loaded => {
                                    shell.app.state.pckg.pending_catalog_refresh = true;
                                }
                                OverlayKind::Templates
                                    if !shell.app.state.templates.catalog_loaded =>
                                {
                                    shell.app.state.templates.pending_catalog_refresh = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    redraw_signal_request(application);
                    dirty = true;
                    if let Some(ack) = ack {
                        let _ = ack.send(());
                    }
                }
                RuntimeOp::Suspend(ack) => {
                    if !suspended {
                        let _ = application.lock_ports();
                        terminal.leave_alternate_screen().map_err(runtime_err)?;
                        terminal.disable_raw_mode().map_err(runtime_err)?;
                        terminal.disable_mouse_capture().map_err(runtime_err)?;
                        shutdown_session_logger();
                        suspended = true;
                    }
                    let _ = ack.send(());
                }
                RuntimeOp::Resume(ack) => {
                    if suspended {
                        terminal.enable_raw_mode().map_err(runtime_err)?;
                        terminal.enter_alternate_screen().map_err(runtime_err)?;
                        terminal.enable_mouse_capture().map_err(runtime_err)?;
                        let _ = application.unlock_ports();
                        init_session_logger();
                        let size = terminal.raw().size().map_err(io::Error::other)?;
                        shell_mut(application).handle_shell_event(ShellRealmEvent::Resize {
                            width: size.width,
                            height: size.height,
                        });
                        suspended = false;
                        dirty = true;
                    }
                    let _ = ack.send(());
                }
                RuntimeOp::WaitFocus { target, ack } => {
                    let state = &mut shell_mut(application).app.state;
                    state.awaiting_nav = Some(target);
                    if state.nav_reached(target) {
                        state.awaiting_nav = None;
                        let _ = ack.send(());
                    } else {
                        pending_focus.push((target, ack));
                    }
                    dirty = true;
                }
                RuntimeOp::WaitDismiss(ack) => {
                    {
                        let shell = shell_mut(application);
                        shell
                            .app
                            .state
                            .set_overlay_visible(OverlayKind::Summary, true);
                        shell.app.state.focus_overlay(OverlayKind::Summary);
                        shell.app.state.sync_summary_explorer();
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
            let state = &shell_mut(application).app.state;
            if state.nav_reached(*target) {
                shell_mut(application).app.state.awaiting_nav = None;
                let _ = ack.send(());
                false
            } else {
                true
            }
        });

        let mut input_action = InputAction::None;

        if !suspended {
            match application.tick(PollStrategy::TryFor(TICK)) {
                Ok(messages) => {
                    for msg in messages {
                        match msg {
                            PipelineShellMsg::Quit => {
                                quitting = true;
                                dirty = true;
                            }
                            PipelineShellMsg::Input => {
                                let result = shell_mut(application).app.take_input_result();
                                if let Some(result) = result {
                                    input_action = apply_input_result(
                                        result,
                                        &interrupt,
                                        &mut shell_mut(application).app.state,
                                        &mut pending_focus,
                                        &mut pending_dismiss,
                                        &mut quitting,
                                    );
                                }
                                dirty = true;
                            }
                            PipelineShellMsg::Redraw => dirty = true,
                        }
                    }
                }
                Err(err) => return Err(io::Error::other(err.to_string())),
            }

            if last_tick.elapsed() >= TICK {
                let effects = shell_mut(application)
                    .app
                    .apply_message(&ShellMessage::Tick);
                apply_effects(effects, &tx, &mut shell_mut(application).app.state);
                last_tick = Instant::now();
                dirty = true;
            }

            drain_pending_work(&tx, &mut shell_mut(application).app.state);

            if shell_mut(application)
                .app
                .redraw_signal
                .take_redraw_request()
            {
                dirty = true;
            }
        } else {
            thread::sleep(TICK);
        }

        {
            let state = &mut shell_mut(application).app.state;
            if input_action == InputAction::Quit {
                request_quit(&interrupt, state, &mut pending_focus, &mut pending_dismiss);
                quitting = true;
                dirty = true;
            } else {
                resolve_waits(
                    &mut pending_focus,
                    &mut pending_dismiss,
                    state,
                    input_action,
                );
            }
        }

        if dirty && !quitting && !suspended {
            crate::pipeline::tui::reset_stderr_ansi()?;
            terminal
                .draw(|frame| {
                    application.view(&PipelineShellId::Root, frame, frame.area());
                })
                .map_err(runtime_err)?;
            dirty = false;
        }
    }

    Ok(())
}

fn apply_runtime_message(
    application: &mut Application<PipelineShellId, PipelineShellMsg, NoUserEvent>,
    msg: &ShellMessage,
    tx: &Sender<RuntimeOp>,
) {
    let shell = shell_mut(application);
    let effects = shell.app.apply_message(msg);
    apply_effects(effects, tx, &mut shell.app.state);
}

fn redraw_signal_request(
    application: &mut Application<PipelineShellId, PipelineShellMsg, NoUserEvent>,
) {
    shell_mut(application).app.redraw_signal.request_redraw();
}

fn runtime_err(err: impl std::fmt::Display) -> io::Error {
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
