//! Pipeline shell root component (tuirealm `AppComponent`).

use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, NoUserEvent};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::ratatui::layout::Rect;
use tuirealm::state::State;

use crate::tui::app::BeskidShellApp;
use crate::tui::realm::shell_event::{ShellOutcome, shell_event_from_realm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineShellId {
    Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineShellMsg {
    Redraw,
    Quit,
    Input,
}

pub struct PipelineShellComponent {
    pub app: BeskidShellApp,
}

impl PipelineShellComponent {
    pub fn new(app: BeskidShellApp) -> Self {
        Self { app }
    }

    pub fn handle_shell_event(
        &mut self,
        event: crate::tui::realm::shell_event::ShellRealmEvent,
    ) -> PipelineShellMsg {
        match self.app.handle_shell_event(event) {
            ShellOutcome::Quit => PipelineShellMsg::Quit,
            ShellOutcome::Redraw | ShellOutcome::Continue => {
                if self.app.take_input_result().is_some() {
                    PipelineShellMsg::Input
                } else {
                    PipelineShellMsg::Redraw
                }
            }
        }
    }
}

impl Component for PipelineShellComponent {
    fn view(&mut self, frame: &mut Frame, _area: Rect) {
        self.app.draw(frame);
    }

    fn query<'a>(&'a self, _attr: Attribute) -> Option<QueryResult<'a>> {
        None
    }

    fn attr(&mut self, _attr: Attribute, _value: AttrValue) {}

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<PipelineShellMsg, NoUserEvent> for PipelineShellComponent {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<PipelineShellMsg> {
        let shell_event = shell_event_from_realm(ev)?;
        Some(self.handle_shell_event(shell_event))
    }
}

impl PipelineShellComponent {
    pub fn as_any_mut(
        component: &mut dyn AppComponent<PipelineShellMsg, NoUserEvent>,
    ) -> &mut Self {
        component
            .as_any_mut()
            .downcast_mut::<Self>()
            .expect("pipeline shell component")
    }
}
