//! Command catalog: CLI subprocess commands, nav menu items, and contextual commands.

use super::layout::pages::PagesDoc;
use super::nav::{NavAction, NavItemDescriptor, NavRegistry};
use super::scope::ShellScope;
use super::workflow::WorkflowStage;

/// How a palette entry is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Workflow,
    Contextual,
    Nav,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowCommandDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub stage: WorkflowStage,
    pub args_hint: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextualCommand {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub args_hint: Option<&'static str>,
    pub widget_id: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavCommandDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub action: NavAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandItem {
    Workflow(WorkflowCommandDef),
    Contextual(ContextualCommand),
    Nav(NavCommandDef),
}

impl CommandItem {
    pub fn kind(&self) -> CommandKind {
        match self {
            Self::Workflow(_) => CommandKind::Workflow,
            Self::Contextual(_) => CommandKind::Contextual,
            Self::Nav(_) => CommandKind::Nav,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Workflow(c) => c.id,
            Self::Contextual(c) => c.id,
            Self::Nav(c) => c.id.as_str(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Workflow(c) => c.name,
            Self::Contextual(c) => c.name,
            Self::Nav(c) => c.name.as_str(),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Workflow(c) => c.description,
            Self::Contextual(c) => c.description,
            Self::Nav(c) => c.description.as_str(),
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Workflow(c) => c.icon,
            Self::Contextual(c) => c.icon,
            Self::Nav(c) => c.icon.as_str(),
        }
    }

    pub fn args_hint(&self) -> Option<&str> {
        match self {
            Self::Workflow(c) => Some(c.args_hint),
            Self::Contextual(c) => c.args_hint,
            Self::Nav(_) => None,
        }
    }
}

/// Flatten the navigation tree into palette entries (groups omitted).
pub fn nav_palette_commands(registry: &NavRegistry, pages: &PagesDoc) -> Vec<CommandItem> {
    let mut merged = NavRegistry::new();
    merged.merge_pages(pages);
    for item in registry.items() {
        merged.register(item.clone());
    }

    let mut roots: Vec<_> = merged.roots().into_iter().cloned().collect();
    roots.sort_by_key(|item| item.order);

    let mut out = Vec::new();
    for root in roots {
        append_nav_palette(&mut out, &merged, &root, Vec::new());
    }
    out
}

fn append_nav_palette(
    out: &mut Vec<CommandItem>,
    registry: &NavRegistry,
    item: &NavItemDescriptor,
    trail: Vec<String>,
) {
    let mut path = trail;
    path.push(item.label.clone());
    if !matches!(item.action, NavAction::Group) {
        let breadcrumb = path.join(" › ");
        out.push(CommandItem::Nav(NavCommandDef {
            id: format!("nav.{}", item.id),
            name: breadcrumb,
            description: item.label.clone(),
            icon: item.icon.clone().unwrap_or_else(|| "›".into()),
            action: item.action.clone(),
        }));
    }
    let mut children: Vec<_> = registry.children_of(&item.id).into_iter().cloned().collect();
    children.sort_by_key(|child| child.order);
    for child in children {
        append_nav_palette(out, registry, &child, path.clone());
    }
}

/// Built-in workflow commands.
pub fn builtin_workflow_commands() -> Vec<CommandItem> {
    vec![
        CommandItem::Workflow(WorkflowCommandDef {
            id: "build",
            name: "build",
            description: "AOT compile",
            icon: "⚙",
            stage: WorkflowStage::Build,
            args_hint: "[file]",
        }),
        CommandItem::Workflow(WorkflowCommandDef {
            id: "test",
            name: "test",
            description: "Run test targets",
            icon: "✓",
            stage: WorkflowStage::Test,
            args_hint: "[file]",
        }),
        CommandItem::Workflow(WorkflowCommandDef {
            id: "run",
            name: "run",
            description: "Compile and execute",
            icon: "▶",
            stage: WorkflowStage::Run,
            args_hint: "[file]",
        }),
        CommandItem::Workflow(WorkflowCommandDef {
            id: "analyze",
            name: "analyze",
            description: "Semantic analysis",
            icon: "◇",
            stage: WorkflowStage::Analyze,
            args_hint: "[file]",
        }),
        CommandItem::Workflow(WorkflowCommandDef {
            id: "graph",
            name: "graph",
            description: "Dependency graph",
            icon: "◎",
            stage: WorkflowStage::Graph,
            args_hint: "",
        }),
    ]
}

pub fn builtin_contextual_commands(scope: &ShellScope) -> Vec<CommandItem> {
    let mut out = vec![
        contextual("ctx.palette", "Command palette", "Open command palette", "⌘", None, None),
        contextual("ctx.pckg", "Browse packages", "Open pckg browser", "📦", None, Some("pckg.browser")),
        contextual("ctx.templates", "New project", "Open template picker", "＋", None, Some("templates.picker")),
        contextual(
            "ctx.open_workspace",
            "Open workspace",
            "Pick a workspace (.bws) to scope the shell",
            "◎",
            None,
            None,
        ),
        contextual("ctx.open_project", "Open project", "Pick a project (.bproj) to scope the shell", "▣", None, None),
        contextual("ctx.layout_edit", "Layout edit", "Toggle layout editor", "▦", None, None),
    ];
    match scope {
        ShellScope::Workspace { .. } | ShellScope::Project { .. } => {
            out.push(contextual(
                "ctx.graph",
                "Dependency graph",
                "Show workspace/project graph",
                "◎",
                None,
                Some("graph.deps"),
            ));
            out.push(contextual("ctx.tests", "Run tests", "Run tests in scope", "✓", None, Some("tests.runner")));
            out.push(contextual(
                "ctx.analyze",
                "Analyze",
                "Run semantic analysis",
                "◇",
                None,
                Some("analysis.diagnostics"),
            ));
        }
        ShellScope::User => {}
    }
    out.into_iter().map(CommandItem::Contextual).collect()
}

/// Palette entries when layout editor is active or inactive.
pub fn layout_editor_commands(edit_active: bool) -> Vec<CommandItem> {
    if !edit_active {
        return vec![CommandItem::Contextual(contextual(
            "ctx.layout_edit",
            "Layout edit",
            "Toggle layout editor",
            "▦",
            None,
            None,
        ))];
    }
    vec![
        CommandItem::Contextual(contextual(
            "layout.focus_next",
            "Focus next panel",
            "Select next layout panel",
            "↓",
            None,
            None,
        )),
        CommandItem::Contextual(contextual(
            "layout.focus_prev",
            "Focus prev panel",
            "Select previous layout panel",
            "↑",
            None,
            None,
        )),
        CommandItem::Contextual(contextual(
            "layout.add",
            "Add panel",
            "Add widget panel (param: widget id)",
            "+",
            Some("<widget>"),
            None,
        )),
        CommandItem::Contextual(contextual("layout.remove", "Remove panel", "Remove focused panel", "−", None, None)),
        CommandItem::Contextual(contextual(
            "layout.wrap_col",
            "Wrap column",
            "Wrap focused panel in column",
            "⫯",
            None,
            None,
        )),
        CommandItem::Contextual(contextual(
            "layout.wrap_row",
            "Wrap row",
            "Wrap focused panel in row",
            "⫰",
            None,
            None,
        )),
        CommandItem::Contextual(contextual("layout.tabs", "Convert tabs", "Convert layout to tabs", "⊞", None, None)),
        CommandItem::Contextual(contextual(
            "layout.stack",
            "Convert stack",
            "Convert layout to stack",
            "▤",
            None,
            None,
        )),
        CommandItem::Contextual(contextual(
            "layout.set_widget",
            "Set widget",
            "Set focused panel widget id",
            "◎",
            Some("<widget>"),
            None,
        )),
        CommandItem::Contextual(contextual("layout.save", "Save layout", "Save board to scope path", "💾", None, None)),
        CommandItem::Contextual(contextual(
            "layout.reset",
            "Reset layout",
            "Reset to embedded default",
            "↺",
            None,
            None,
        )),
    ]
}

/// Shared palette and demoted top-menu command list.
pub fn command_catalog(
    scope: &ShellScope,
    layout_edit_active: bool,
    nav: &NavRegistry,
    pages: &PagesDoc,
) -> Vec<CommandItem> {
    let mut items = nav_palette_commands(nav, pages);
    items.extend(builtin_workflow_commands());
    items.extend(builtin_contextual_commands(scope));
    items.extend(layout_editor_commands(layout_edit_active));
    items
}

fn contextual(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    args_hint: Option<&'static str>,
    widget_id: Option<&'static str>,
) -> ContextualCommand {
    ContextualCommand { id, name, description, icon, args_hint, widget_id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::layout::pages::{EMBEDDED_HI_PAGES, parse_pages};
    use crate::shell::nav::NavRegistry;
    use crate::shell::scope::ShellScope;

    #[test]
    fn command_catalog_includes_nav_and_workflow() {
        let registry = NavRegistry::new();
        let pages = parse_pages(EMBEDDED_HI_PAGES).expect("pages");
        let scope = ShellScope::User;
        let items = command_catalog(&scope, false, &registry, &pages);
        assert!(items.iter().any(|item| item.name().contains("Graphs")));
        assert!(items.iter().any(|item| item.id() == "build"));
    }

    #[test]
    fn nav_palette_includes_selectable_pages() {
        let registry = NavRegistry::new();
        let pages = parse_pages(EMBEDDED_HI_PAGES).expect("pages");
        let items = nav_palette_commands(&registry, &pages);
        assert!(items.iter().any(|item| item.name().contains("Graphs")));
        assert!(!items.iter().any(|item| item.name() == "Beskid"));
    }
}
