//! Space-driven screen navigation after pipeline / test phases complete.

use super::model::{Mode, Model};

/// Which screen the user is waiting to advance to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    Tests,
    Summary,
    Exit,
}

impl Model {
    pub fn navigation_hint(&self) -> Option<&'static str> {
        match self.mode {
            Mode::Pipeline if self.tests_loaded => Some("[Space] tests"),
            Mode::Tests if self.summary_ready => Some("[Space] summary"),
            Mode::Summary => Some("[Space/q] exit"),
            Mode::Pipeline if self.summary_ready => Some("[Space] summary"),
            _ => None,
        }
    }

    pub fn can_advance_to(&self, target: NavTarget) -> bool {
        match target {
            NavTarget::Tests => self.mode == Mode::Pipeline && self.tests_loaded,
            NavTarget::Summary => {
                (self.mode == Mode::Tests || self.mode == Mode::Pipeline) && self.summary_ready
            }
            NavTarget::Exit => self.mode == Mode::Summary,
        }
    }

    pub fn advance_once(&mut self) -> Option<NavTarget> {
        match self.mode {
            Mode::Pipeline if self.tests_loaded => {
                self.mode = Mode::Tests;
                None
            }
            Mode::Pipeline if self.summary_ready => {
                self.mode = Mode::Summary;
                None
            }
            Mode::Tests if self.summary_ready => {
                self.mode = Mode::Summary;
                None
            }
            Mode::Summary => Some(NavTarget::Exit),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::Model;

    #[test]
    fn advance_pipeline_to_tests_then_summary() {
        let mut model = Model::default();
        model.tests_loaded = true;
        assert_eq!(model.mode, Mode::Pipeline);
        assert!(model.navigation_hint().is_some());
        assert!(model.advance_once().is_none());
        assert_eq!(model.mode, Mode::Tests);
        model.summary_ready = true;
        assert!(model.advance_once().is_none());
        assert_eq!(model.mode, Mode::Summary);
        assert_eq!(model.advance_once(), Some(NavTarget::Exit));
    }
}
