//! Ratkit runtime re-exports (vendored ratkit 0.2.16 + ratatui 0.30).

use crossterm::event::Event;

pub use ratkit::{
    CoordinatorAction, CoordinatorApp, CoordinatorEvent, KeyboardEvent, LayoutError, LayoutResult,
    MouseEvent, RedrawSignal, ResizeEvent, Runner, RunnerAction, RunnerConfig, RunnerEvent,
    TickEvent,
};

/// Returns true when the runner requests application exit.
pub fn map_runner_action(action: RunnerAction) -> bool {
    matches!(action, RunnerAction::Quit)
}

/// Map a crossterm event into a ratkit runner event when applicable.
pub fn runner_event_from_crossterm(event: Event) -> Option<RunnerEvent> {
    match event {
        Event::Key(key) => Some(RunnerEvent::Keyboard(KeyboardEvent::from_crossterm(key))),
        Event::Mouse(mouse) => Some(RunnerEvent::Mouse(MouseEvent::from_crossterm(mouse))),
        Event::Resize(width, height) => Some(RunnerEvent::Resize(ResizeEvent::new(width, height))),
        _ => None,
    }
}

/// Build a tick runner event for the background loop.
pub fn tick_event(count: u64) -> RunnerEvent {
    RunnerEvent::Tick(TickEvent::new(count))
}
