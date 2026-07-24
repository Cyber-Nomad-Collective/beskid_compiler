//! Map pipeline stage labels to layout focus (pane weights and primary content).

use crate::tui::shell::focus::{FocusTarget, OverlayKind};
use crate::tui::shell::state::ShellState;

/// Which pipeline region drives pane sizing and primary-panel content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageFocus {
    Workspace,
    FrontEnd,
    Semantic,
    LowerCodegen,
    Tests,
    Summary,
}

impl StageFocus {
    /// Left (primary) pane percentage in the main horizontal split.
    #[allow(dead_code)]
    pub fn main_split_left_pct(self) -> u16 {
        match self {
            Self::Semantic => 36,
            Self::Workspace => 46,
            Self::FrontEnd => 40,
            Self::LowerCodegen => 42,
            Self::Tests => 34,
            Self::Summary => 42,
        }
    }

    /// Minimum log panel height (rows).
    #[allow(dead_code)]
    pub fn log_min_rows(self) -> u16 {
        match self {
            Self::Semantic | Self::LowerCodegen => 7,
            Self::Summary => 6,
            _ => 6,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace",
            Self::FrontEnd => "Front end",
            Self::Semantic => "Semantic analysis",
            Self::LowerCodegen => "Lowering & codegen",
            Self::Tests => "Tests",
            Self::Summary => "Summary",
        }
    }

    /// Gray helper copy for the primary pane when a stage has little live detail.
    pub fn description(self) -> &'static str {
        match self {
            Self::Workspace => "Dependency resolution and materialization steps update here.",
            Self::FrontEnd => "Per-unit parse and macro expansion progress streams in the log.",
            Self::Semantic => "Nested semantic rules and type-check passes appear in the pipeline tree.",
            Self::LowerCodegen => "Lowering, CLIF, and JIT/AOT emit phases drive this stage.",
            Self::Tests => "Each test row updates as cases start, pass, or fail. Full output streams in the log panel.",
            Self::Summary => "Final pass/fail counts, timings, and charts land here when the command finishes.",
        }
    }

    pub fn from_stage_label(label: &str) -> Self {
        let lower = label.to_ascii_lowercase();
        if lower.contains("resolve")
            || lower.contains("materialize")
            || lower.contains("assemble")
            || lower.contains("dependency")
            || lower.contains("workspace")
            || lower.contains("copy ")
            || lower.contains("fetch registry")
            || lower.contains("lockfile")
        {
            return Self::Workspace;
        }
        if lower.starts_with("parse") || lower.contains("expand macro") || lower.contains("macro") {
            return Self::FrontEnd;
        }
        if lower.contains("semantic")
            || lower.contains("lower ast")
            || lower.contains("collect definition")
            || lower.contains("control flow")
            || lower.contains("resolve name")
            || lower.contains("visibility")
            || lower.contains("contract")
            || lower.contains("error handling")
            || (lower.contains("type check") && !lower.contains("jit"))
            || lower.contains("normalize hir")
            || lower.contains("resolve (pass")
        {
            return Self::Semantic;
        }
        Self::LowerCodegen
    }

    pub fn from_shell_state(state: &ShellState) -> Self {
        match state.focus {
            FocusTarget::Overlay(OverlayKind::Tests) => Self::Tests,
            FocusTarget::Overlay(OverlayKind::Summary) => Self::Summary,
            FocusTarget::Overlay(OverlayKind::Pckg)
            | FocusTarget::Overlay(OverlayKind::Templates)
            | FocusTarget::Overlay(OverlayKind::CompileDebug)
            | FocusTarget::Overlay(OverlayKind::Graph)
            | FocusTarget::Overlay(OverlayKind::Settings)
            | FocusTarget::Overlay(OverlayKind::Analysis) => Self::Workspace,
            FocusTarget::Base(_) => Self::from_stage_label(&state.pipeline.stage_label),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_semantic_type_check() {
        assert_eq!(StageFocus::from_stage_label("Type check"), StageFocus::Semantic);
    }

    #[test]
    fn classifies_resolve_manifest() {
        assert_eq!(StageFocus::from_stage_label("Resolve manifest"), StageFocus::Workspace);
    }

    #[test]
    fn classifies_jit_emit() {
        assert_eq!(StageFocus::from_stage_label("JIT compile"), StageFocus::LowerCodegen);
    }

    #[test]
    fn each_focus_has_description() {
        let focuses = [
            StageFocus::Workspace,
            StageFocus::FrontEnd,
            StageFocus::Semantic,
            StageFocus::LowerCodegen,
            StageFocus::Tests,
            StageFocus::Summary,
        ];
        for focus in focuses {
            let desc = focus.description();
            assert!(!desc.is_empty(), "{focus:?}");
            assert!(desc.len() > 20, "{focus:?}");
        }
    }

    #[test]
    fn workspace_description_mentions_materialization() {
        assert!(StageFocus::Workspace.description().contains("materialization"));
    }

    #[test]
    fn tests_description_mentions_log_panel() {
        assert!(StageFocus::Tests.description().contains("log panel"));
    }
}
