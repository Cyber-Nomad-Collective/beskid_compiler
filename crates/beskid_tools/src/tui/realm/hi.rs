//! `beskid hi` tuirealm event loop on stderr.

use std::io;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;

use tuirealm::application::{Application, PollStrategy};
use tuirealm::event::NoUserEvent;
use tuirealm::listener::EventListenerCfg;
use tuirealm::terminal::TerminalAdapter;

use crate::shell::cli_run::run_cli_plan;
use crate::shell::host::HiShellApp;
use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent};
use crate::tui::realm::stderr_adapter::StderrTerminalAdapter;

const TICK: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HiShellId {
    Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiShellMsg {
    Redraw,
    Quit,
}

struct HiShellComponent {
    app: HiShellApp,
}

impl HiShellComponent {
    fn handle_shell_event(&mut self, event: ShellRealmEvent) -> HiShellMsg {
        match self.app.handle_shell_event(event) {
            ShellOutcome::Quit => HiShellMsg::Quit,
            ShellOutcome::Redraw | ShellOutcome::Continue => HiShellMsg::Redraw,
        }
    }
}

impl tuirealm::component::Component for HiShellComponent {
    fn view(&mut self, frame: &mut tuirealm::ratatui::Frame, area: tuirealm::ratatui::layout::Rect) {
        self.app.set_frame_area(area);
        self.app.draw_shell(frame);
    }

    fn query<'a>(
        &'a self,
        _attr: tuirealm::props::Attribute,
    ) -> Option<tuirealm::props::QueryResult<'a>> {
        None
    }

    fn attr(&mut self, _attr: tuirealm::props::Attribute, _value: tuirealm::props::AttrValue) {}

    fn state(&self) -> tuirealm::state::State {
        tuirealm::state::State::None
    }

    fn perform(&mut self, _cmd: tuirealm::command::Cmd) -> tuirealm::command::CmdResult {
        tuirealm::command::CmdResult::NoChange
    }
}

impl tuirealm::component::AppComponent<HiShellMsg, NoUserEvent> for HiShellComponent {
    fn on(&mut self, ev: &tuirealm::event::Event<NoUserEvent>) -> Option<HiShellMsg> {
        let shell_event = crate::tui::realm::shell_event::shell_event_from_realm(ev)?;
        Some(self.handle_shell_event(shell_event))
    }
}

/// Run the hi shell until quit (stderr alternate screen).
pub fn run_hi(app: HiShellApp) -> io::Result<()> {
    let listener = EventListenerCfg::default()
        .crossterm_input_listener(Duration::from_millis(10), 3)
        .tick_interval(TICK);

    let mut application = Application::init(listener);
    application
        .mount(HiShellId::Root, Box::new(HiShellComponent { app }), Vec::new())
        .map_err(|err| io::Error::other(err.to_string()))?;
    application
        .active(&HiShellId::Root)
        .map_err(|err| io::Error::other(err.to_string()))?;

    let mut terminal = StderrTerminalAdapter::new()
        .map_err(|err| io::Error::other(err.to_string()))?;
    terminal
        .enable_raw_mode()
        .map_err(|err| io::Error::other(err.to_string()))?;
    terminal
        .enter_alternate_screen()
        .map_err(|err| io::Error::other(err.to_string()))?;
    terminal
        .enable_mouse_capture()
        .map_err(|err| io::Error::other(err.to_string()))?;

    let size = terminal.raw().size().map_err(io::Error::other)?;
    if let Some(component) = application.get_component_mut(&HiShellId::Root) {
        let inner = component
            .as_any_mut()
            .downcast_mut::<HiShellComponent>()
            .expect("hi shell component");
        let _ = inner.handle_shell_event(ShellRealmEvent::Resize {
            width: size.width,
            height: size.height,
        });
    }

    let mut quitting = false;
    let mut dirty = true;
    let mut compile_thread: Option<JoinHandle<Result<()>>> = None;

    while !quitting {
        match application.tick(PollStrategy::TryFor(TICK)) {
            Ok(messages) => {
                for msg in messages {
                    if msg == HiShellMsg::Quit {
                        quitting = true;
                    }
                    dirty = true;
                }
            }
            Err(err) => return Err(io::Error::other(err.to_string())),
        }

        if let Some(component) = application.get_component_mut(&HiShellId::Root) {
            let inner = component
                .as_any_mut()
                .downcast_mut::<HiShellComponent>()
                .expect("hi shell component");

            if let Some(handle) = compile_thread.as_ref() {
                if handle.is_finished() {
                    let handle = compile_thread.take().expect("compile thread");
                    let result = handle
                        .join()
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("compile thread panicked")));
                    inner.app.on_compile_finished(result);
                    dirty = true;
                }
            }

            if compile_thread.is_none() {
                if let Some(job) = inner.app.take_pending_compile() {
                    compile_thread = inner.app.spawn_compile_job(job);
                    dirty = true;
                }
            }

            if let Some(plan) = inner.app.take_pending_cli() {
                terminal
                    .suspend_for_subprocess()
                    .map_err(|err| io::Error::other(err.to_string()))?;
                let _ = run_cli_plan(&plan);
                terminal
                    .resume_after_subprocess()
                    .map_err(|err| io::Error::other(err.to_string()))?;
                let size = terminal.raw().size().map_err(io::Error::other)?;
                let _ = inner.handle_shell_event(ShellRealmEvent::Resize {
                    width: size.width,
                    height: size.height,
                });
                dirty = true;
            }
        }

        if dirty {
            terminal
                .draw(|frame| {
                    application.view(&HiShellId::Root, frame, frame.area());
                })
                .map_err(|err| io::Error::other(err.to_string()))?;
            dirty = false;
        }
    }

    terminal
        .restore()
        .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(())
}
