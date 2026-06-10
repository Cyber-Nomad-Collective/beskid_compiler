//! Extension navigation catalog entries.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionNavAction {
    Group,
    Page(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionNavItem {
    pub id: &'static str,
    pub label: &'static str,
    pub action: ExtensionNavAction,
    pub parent: Option<&'static str>,
    pub order: u32,
    pub icon: Option<&'static str>,
}

pub const NAV_CATALOG: &[ExtensionNavItem] = &[ExtensionNavItem {
    id: "hi",
    label: "Hi extensions",
    action: ExtensionNavAction::Group,
    parent: Some("beskid"),
    order: 3,
    icon: Some("★"),
}];
