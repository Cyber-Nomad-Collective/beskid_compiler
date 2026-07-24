//! Extension widget catalog descriptors.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionWidgetDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    pub default_grow: Option<u32>,
}

impl ExtensionWidgetDescriptor {
    pub const fn new(
        id: &'static str,
        title: &'static str,
        icon: &'static str,
        description: &'static str,
        default_grow: Option<u32>,
    ) -> Self {
        Self { id, title, icon, description, default_grow }
    }
}

pub const WIDGET_CATALOG: &[ExtensionWidgetDescriptor] =
    &[ExtensionWidgetDescriptor::new("hi.hello", "Hello", "★", "beskid_hi extension demo tile", Some(1))];
