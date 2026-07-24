//! Register extension widgets with the shell host.

use beskid_tools::shell::nav::NavAction;
use beskid_tools::shell::{NavItemDescriptor, NavRegistry, WidgetDescriptor, WidgetRegistry};

use crate::models::descriptor::WIDGET_CATALOG;
use crate::models::nav::NAV_CATALOG;
use crate::widgets::HelloWidget;

pub fn register_widgets(registry: &mut WidgetRegistry) {
    for desc in WIDGET_CATALOG {
        registry.register_descriptor(WidgetDescriptor::new(
            desc.id,
            desc.title,
            desc.icon,
            desc.description,
            desc.default_grow,
        ));
    }
    registry.register(Box::new(HelloWidget::new()));
}

pub fn register_nav(registry: &mut NavRegistry) {
    for item in NAV_CATALOG {
        registry.register(NavItemDescriptor {
            id: item.id.into(),
            label: item.label.into(),
            action: match item.action {
                crate::models::nav::ExtensionNavAction::Group => NavAction::Group,
                crate::models::nav::ExtensionNavAction::Page(id) => NavAction::Page(id.into()),
            },
            parent: item.parent.map(str::to_string),
            order: item.order,
            icon: item.icon.map(str::to_string),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_tools::shell::WidgetRegistry;

    #[test]
    fn catalog_ids_match_registered_widgets() {
        let mut registry = WidgetRegistry::new();
        register_widgets(&mut registry);
        for desc in WIDGET_CATALOG {
            assert!(registry.get(desc.id).is_some(), "missing widget for catalog id {}", desc.id);
        }
    }
}
