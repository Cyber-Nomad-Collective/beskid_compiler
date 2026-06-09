//! Command catalog: CLI subprocess commands and contextual in-shell commands.

use super::scope::ShellScope;

/// How a palette entry is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Cli,
    Contextual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CliCommandDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub argv_prefix: &'static [&'static str],
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
pub enum CommandItem {
    Cli(CliCommandDef),
    Contextual(ContextualCommand),
}

impl CommandItem {
    pub fn kind(&self) -> CommandKind {
        match self {
            Self::Cli(_) => CommandKind::Cli,
            Self::Contextual(_) => CommandKind::Contextual,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Cli(c) => c.id,
            Self::Contextual(c) => c.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Cli(c) => c.name,
            Self::Contextual(c) => c.name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Cli(c) => c.description,
            Self::Contextual(c) => c.description,
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Cli(c) => c.icon,
            Self::Contextual(c) => c.icon,
        }
    }

    pub fn args_hint(&self) -> Option<&str> {
        match self {
            Self::Cli(c) => Some(c.args_hint),
            Self::Contextual(c) => c.args_hint,
        }
    }
}

/// Built-in CLI commands mirrored from `beskid_cli`.
pub fn builtin_cli_commands() -> Vec<CommandItem> {
    vec![
        cli("analyze", "analyze", "Semantic analysis", "◇", &["analyze"], "[file]"),
        cli("build", "build", "AOT compile and link", "⚙", &["build"], "[file]"),
        cli("run", "run", "Compile and execute", "▶", &["run"], "[file]"),
        cli("test", "test", "Run test targets", "✓", &["test"], "[file]"),
        cli("fetch", "fetch", "Resolve dependencies", "↓", &["fetch"], ""),
        cli("lock", "lock", "Sync lockfile", "⎘", &["lock"], ""),
        cli("update", "update", "Update dependencies", "↻", &["update"], ""),
        cli("graph", "graph", "Dependency graph", "◎", &["graph"], ""),
        cli("pckg", "pckg", "Package registry", "📦", &["pckg"], "subcommand"),
        cli("new", "new", "Scaffold from template", "＋", &["new"], "[name]"),
        cli("fmt", "fmt", "Format sources", "≡", &["fmt"], "[path]"),
        cli("repl", "repl", "Interactive REPL", "❯", &["repl"], ""),
        cli("lsp", "lsp", "Language server", "⌁", &["lsp"], ""),
    ]
}

pub fn builtin_contextual_commands(scope: &ShellScope) -> Vec<CommandItem> {
    let mut out = vec![
        contextual(
            "ctx.palette",
            "Command palette",
            "Open command palette",
            "⌘",
            None,
            None,
        ),
        contextual(
            "ctx.pckg",
            "Browse packages",
            "Open pckg browser",
            "📦",
            None,
            Some("pckg.browser"),
        ),
        contextual(
            "ctx.templates",
            "New project",
            "Open template picker",
            "＋",
            None,
            Some("templates.picker"),
        ),
        contextual(
            "ctx.open_workspace",
            "Open workspace",
            "Pick a workspace (.bws) to scope the shell",
            "◎",
            None,
            None,
        ),
        contextual(
            "ctx.open_project",
            "Open project",
            "Pick a project (.bproj) to scope the shell",
            "▣",
            None,
            None,
        ),
        contextual(
            "ctx.layout_edit",
            "Layout edit",
            "Toggle layout editor",
            "▦",
            None,
            None,
        ),
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
            out.push(contextual(
                "ctx.tests",
                "Run tests",
                "Run tests in scope",
                "✓",
                None,
                Some("tests.runner"),
            ));
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
        CommandItem::Contextual(contextual("layout.focus_next", "Focus next panel", "Select next layout panel", "↓", None, None)),
        CommandItem::Contextual(contextual("layout.focus_prev", "Focus prev panel", "Select previous layout panel", "↑", None, None)),
        CommandItem::Contextual(contextual("layout.add", "Add panel", "Add widget panel (param: widget id)", "+", Some("<widget>"), None)),
        CommandItem::Contextual(contextual("layout.remove", "Remove panel", "Remove focused panel", "−", None, None)),
        CommandItem::Contextual(contextual("layout.wrap_col", "Wrap column", "Wrap focused panel in column", "⫯", None, None)),
        CommandItem::Contextual(contextual("layout.wrap_row", "Wrap row", "Wrap focused panel in row", "⫰", None, None)),
        CommandItem::Contextual(contextual("layout.tabs", "Convert tabs", "Convert layout to tabs", "⊞", None, None)),
        CommandItem::Contextual(contextual("layout.stack", "Convert stack", "Convert layout to stack", "▤", None, None)),
        CommandItem::Contextual(contextual("layout.set_widget", "Set widget", "Set focused panel widget id", "◎", Some("<widget>"), None)),
        CommandItem::Contextual(contextual("layout.save", "Save layout", "Save board to scope path", "💾", None, None)),
        CommandItem::Contextual(contextual("layout.reset", "Reset layout", "Reset to embedded default", "↺", None, None)),
    ]
}

fn cli(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    argv_prefix: &'static [&'static str],
    args_hint: &'static str,
) -> CommandItem {
    CommandItem::Cli(CliCommandDef {
        id,
        name,
        description,
        icon,
        argv_prefix,
        args_hint,
    })
}

fn contextual(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    args_hint: Option<&'static str>,
    widget_id: Option<&'static str>,
) -> ContextualCommand {
    ContextualCommand {
        id,
        name,
        description,
        icon,
        args_hint,
        widget_id,
    }
}
