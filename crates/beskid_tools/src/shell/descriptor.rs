//! Widget catalog descriptors for layout editor and palette tooling.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    pub default_grow: Option<u32>,
}

/// Widget kinds pinned outside the layout editor (rendered in shell chrome).
pub const LAYOUT_LOCKED_WIDGETS: &[&str] = &["shell.scope"];

pub fn is_layout_locked_widget(kind: &str) -> bool {
    LAYOUT_LOCKED_WIDGETS.contains(&kind)
}

impl WidgetDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: &'static str,
        title: &'static str,
        icon: &'static str,
        description: &'static str,
        default_grow: Option<u32>,
    ) -> Self {
        Self {
            id,
            title,
            icon,
            description,
            default_grow,
        }
    }
}

/// Built-in widget catalog (mirrors registered builtins).
pub const BUILTIN_DESCRIPTORS: &[WidgetDescriptor] = &[
    WidgetDescriptor::new("shell.scope", "Scope", "◎", "Scope summary header", None),
    WidgetDescriptor::new(
        "hi.welcome",
        "Welcome",
        "◇",
        "Hi dashboard welcome",
        Some(1),
    ),
    WidgetDescriptor::new(
        "shell.shortcuts",
        "Shortcuts",
        "?",
        "Shortcut reference",
        Some(2),
    ),
    WidgetDescriptor::new("shell.log", "Log", "≡", "Log panel", None),
    WidgetDescriptor::new("shell.chrome", "Chrome", "▤", "Footer status", None),
    WidgetDescriptor::new("tests.runner", "Tests", "✓", "Test runner overlay", None),
    WidgetDescriptor::new("pckg.browser", "Packages", "📦", "Package browser", None),
    WidgetDescriptor::new("analysis.diagnostics", "Analyze", "◇", "Diagnostics", None),
    WidgetDescriptor::new("shell.settings", "Settings", "⚙", "Tool settings", Some(2)),
    WidgetDescriptor::new(
        "graph.deps",
        "Dependency graph",
        "◎",
        "Workspace/project graph",
        Some(1),
    ),
    WidgetDescriptor::new(
        "graph.compile",
        "Compile graph",
        "◈",
        "Pipeline phase tree",
        Some(1),
    ),
    WidgetDescriptor::new(
        "compile.debugger",
        "Compile debugger",
        "▣",
        "Phase timeline and traces",
        Some(1),
    ),
    WidgetDescriptor::new(
        "debug.future",
        "Debugger",
        "◉",
        "Compile debugger timeline",
        Some(1),
    ),
    WidgetDescriptor::new(
        "pipeline.stage",
        "Stage",
        "◆",
        "Pipeline stage panel",
        Some(1),
    ),
    WidgetDescriptor::new(
        "pipeline.detail",
        "Detail",
        "▤",
        "Pipeline detail tree",
        Some(2),
    ),
    WidgetDescriptor::new("pipeline.log", "Build log", "≡", "Pipeline log tabs", None),
    WidgetDescriptor::new(
        "pipeline.footer",
        "Progress",
        "▬",
        "Pipeline progress footer",
        None,
    ),
];
