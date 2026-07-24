//! Navigation catalog: built-in items, registration, and merge with pages docs.

use std::collections::HashMap;

use super::layout::pages::{NavItemEntry, PagesDoc};

/// What happens when a nav item is activated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavAction {
    Page(String),
    Overlay(String),
    Widget(String),
    Cli(Vec<String>),
    Group,
}

impl NavAction {
    pub fn from_str(action: &str, target: Option<&str>) -> Result<Self, String> {
        match action {
            "page" => {
                let id = target.ok_or_else(|| "nav page action requires target".to_string())?;
                Ok(Self::Page(id.into()))
            }
            "overlay" => {
                let id = target.ok_or_else(|| "nav overlay action requires target".to_string())?;
                Ok(Self::Overlay(id.into()))
            }
            "widget" => {
                let id = target.ok_or_else(|| "nav widget action requires target".to_string())?;
                Ok(Self::Widget(id.into()))
            }
            "cli" => {
                let raw = target.unwrap_or_default();
                let argv =
                    if raw.is_empty() { Vec::new() } else { raw.split_whitespace().map(str::to_string).collect() };
                Ok(Self::Cli(argv))
            }
            "group" => Ok(Self::Group),
            other => Err(format!("unknown nav action `{other}`")),
        }
    }
}

/// One navigable entry in the hi shell menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItemDescriptor {
    pub id: String,
    pub label: String,
    pub action: NavAction,
    pub parent: Option<String>,
    pub order: u32,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuiltinNavItem {
    id: &'static str,
    label: &'static str,
    action: BuiltinNavAction,
    parent: Option<&'static str>,
    order: u32,
    icon: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum BuiltinNavAction {
    Group,
    Page(&'static str),
    Overlay(&'static str),
    Widget(&'static str),
    Cli(&'static [&'static str]),
}

/// Built-in hierarchical navigation catalog.
pub const BUILTIN_NAV: &[BuiltinNavItem] = &[
    BuiltinNavItem {
        id: "beskid",
        label: "Beskid",
        action: BuiltinNavAction::Group,
        parent: None,
        order: 0,
        icon: Some("◇"),
    },
    BuiltinNavItem {
        id: "compiler",
        label: "Compiler",
        action: BuiltinNavAction::Group,
        parent: Some("beskid"),
        order: 0,
        icon: None,
    },
    BuiltinNavItem {
        id: "graphs",
        label: "Graphs",
        action: BuiltinNavAction::Page("graphs"),
        parent: Some("compiler"),
        order: 0,
        icon: None,
    },
    BuiltinNavItem {
        id: "compile_debug",
        label: "Compile / Debug",
        action: BuiltinNavAction::Page("compile_debug"),
        parent: Some("compiler"),
        order: 1,
        icon: None,
    },
    BuiltinNavItem {
        id: "analysis",
        label: "Analysis",
        action: BuiltinNavAction::Page("analysis"),
        parent: Some("compiler"),
        order: 2,
        icon: None,
    },
    BuiltinNavItem {
        id: "settings",
        label: "Settings",
        action: BuiltinNavAction::Page("settings"),
        parent: Some("compiler"),
        order: 3,
        icon: None,
    },
    BuiltinNavItem {
        id: "debugger",
        label: "Debugger",
        action: BuiltinNavAction::Page("debugger"),
        parent: Some("compiler"),
        order: 4,
        icon: None,
    },
    BuiltinNavItem {
        id: "project",
        label: "Project",
        action: BuiltinNavAction::Group,
        parent: Some("beskid"),
        order: 1,
        icon: None,
    },
    BuiltinNavItem {
        id: "new",
        label: "New project",
        action: BuiltinNavAction::Page("new_project"),
        parent: Some("project"),
        order: 0,
        icon: None,
    },
    BuiltinNavItem {
        id: "boards",
        label: "Boards",
        action: BuiltinNavAction::Page("home"),
        parent: Some("beskid"),
        order: 2,
        icon: None,
    },
];

pub type NavRegistrar = fn(&mut NavRegistry);

pub struct NavRegistry {
    items: HashMap<String, NavItemDescriptor>,
    order: Vec<String>,
}

impl Default for NavRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NavRegistry {
    pub fn new() -> Self {
        let mut registry = Self { items: HashMap::new(), order: Vec::new() };
        registry.register_builtins();
        registry
    }

    pub fn register(&mut self, item: NavItemDescriptor) {
        if !self.items.contains_key(&item.id) {
            self.order.push(item.id.clone());
        }
        self.items.insert(item.id.clone(), item);
    }

    pub fn register_builtins(&mut self) {
        for builtin in BUILTIN_NAV {
            self.register((*builtin).into());
        }
    }

    pub fn merge_pages(&mut self, pages: &PagesDoc) {
        for entry in pages.nav_items.values() {
            self.register(entry.clone().into());
        }
    }

    pub fn items(&self) -> impl Iterator<Item = &NavItemDescriptor> {
        self.order.iter().filter_map(|id| self.items.get(id))
    }

    pub fn get(&self, id: &str) -> Option<&NavItemDescriptor> {
        self.items.get(id)
    }

    pub fn roots(&self) -> Vec<&NavItemDescriptor> {
        self.items().filter(|item| item.parent.is_none()).collect()
    }

    pub fn children_of(&self, parent_id: &str) -> Vec<&NavItemDescriptor> {
        let mut kids: Vec<_> = self.items().filter(|item| item.parent.as_deref() == Some(parent_id)).collect();
        kids.sort_by_key(|item| item.order);
        kids
    }
}

impl From<BuiltinNavItem> for NavItemDescriptor {
    fn from(value: BuiltinNavItem) -> Self {
        let action = match value.action {
            BuiltinNavAction::Group => NavAction::Group,
            BuiltinNavAction::Page(id) => NavAction::Page(id.into()),
            BuiltinNavAction::Overlay(id) => NavAction::Overlay(id.into()),
            BuiltinNavAction::Widget(id) => NavAction::Widget(id.into()),
            BuiltinNavAction::Cli(argv) => NavAction::Cli(argv.iter().map(|s| (*s).into()).collect()),
        };
        Self {
            id: value.id.into(),
            label: value.label.into(),
            action,
            parent: value.parent.map(str::to_string),
            order: value.order,
            icon: value.icon.map(str::to_string),
        }
    }
}

impl From<NavItemEntry> for NavItemDescriptor {
    fn from(value: NavItemEntry) -> Self {
        Self {
            id: value.id,
            label: value.label,
            action: value.action,
            parent: value.parent,
            order: value.order.unwrap_or(0),
            icon: value.icon,
        }
    }
}
