//! Register extension widgets with the shell host.

use beskid_tools::shell::{WidgetDescriptor, WidgetRegistry};

use crate::models::descriptor::WIDGET_CATALOG;
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

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_tools::shell::WidgetRegistry;

    #[test]
    fn catalog_ids_match_registered_widgets() {
        let mut registry = WidgetRegistry::new();
        register_widgets(&mut registry);
        for desc in WIDGET_CATALOG {
            assert!(
                registry.get(desc.id).is_some(),
                "missing widget for catalog id {}",
                desc.id
            );
        }
    }
}
