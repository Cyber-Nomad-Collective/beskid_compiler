//! Keyboard/mouse focus targets for base panels and overlays.

use crate::tui::layout;

/// Which base panel receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneFocus {
    #[default]
    Stage,
    Detail,
    Log,
}

impl PaneFocus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Detail => "detail",
            Self::Log => "log",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Stage => Self::Detail,
            Self::Detail => Self::Log,
            Self::Log => Self::Stage,
        }
    }

    pub fn panel_kind(self) -> &'static str {
        match self {
            Self::Stage => layout::PANEL_STAGE,
            Self::Detail => layout::PANEL_DETAIL,
            Self::Log => layout::PANEL_LOG,
        }
    }
}

/// Floating overlay slots registered on the shell runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    Tests,
    Summary,
    Pckg,
    Templates,
    CompileDebug,
    Graph,
    Settings,
    Analysis,
}

impl std::str::FromStr for OverlayKind {
    type Err = ();

    fn from_str(kind: &str) -> Result<Self, Self::Err> {
        match kind {
            "tests" => Ok(Self::Tests),
            "summary" => Ok(Self::Summary),
            "pckg" => Ok(Self::Pckg),
            "templates" => Ok(Self::Templates),
            "compile_debug" => Ok(Self::CompileDebug),
            "graph" => Ok(Self::Graph),
            "settings" => Ok(Self::Settings),
            "analysis" => Ok(Self::Analysis),
            _ => Err(()),
        }
    }
}

impl OverlayKind {
    pub const ALL: &[OverlayKind] = &[
        Self::Tests,
        Self::Summary,
        Self::Pckg,
        Self::Templates,
        Self::CompileDebug,
        Self::Graph,
        Self::Settings,
        Self::Analysis,
    ];

    pub fn kind_str(self) -> &'static str {
        match self {
            Self::Tests => "tests",
            Self::Summary => "summary",
            Self::Pckg => "pckg",
            Self::Templates => "templates",
            Self::CompileDebug => "compile_debug",
            Self::Graph => "graph",
            Self::Settings => "settings",
            Self::Analysis => "analysis",
        }
    }

    pub fn parse_kind(kind: &str) -> Option<Self> {
        kind.parse().ok()
    }
}

/// Active focus: a base panel or a visible overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Base(PaneFocus),
    Overlay(OverlayKind),
}

impl Default for FocusTarget {
    fn default() -> Self {
        Self::Base(PaneFocus::default())
    }
}

impl FocusTarget {
    pub fn is_overlay(self) -> bool {
        matches!(self, Self::Overlay(_))
    }

    pub fn is_base(self) -> bool {
        matches!(self, Self::Base(_))
    }
}
