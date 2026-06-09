//! Widget registration for the pluggable shell.

use std::collections::HashMap;

use super::catalog::{CommandItem, builtin_cli_commands, builtin_contextual_commands, layout_editor_commands};
use super::descriptor::{BUILTIN_DESCRIPTORS, WidgetDescriptor};
use super::scope::ShellScope;
use super::widget::BeskidWidget;

pub struct WidgetRegistry {
    widgets: HashMap<String, Box<dyn BeskidWidget>>,
    order: Vec<String>,
    descriptors: Vec<WidgetDescriptor>,
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            order: Vec::new(),
            descriptors: BUILTIN_DESCRIPTORS.to_vec(),
        }
    }

    pub fn register(&mut self, widget: Box<dyn BeskidWidget>) {
        let id = widget.meta().id.to_string();
        if !self.widgets.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.widgets.insert(id, widget);
    }

    pub fn register_descriptor(&mut self, descriptor: WidgetDescriptor) {
        if !self.descriptors.iter().any(|d| d.id == descriptor.id) {
            self.descriptors.push(descriptor);
        }
    }

    pub fn descriptors(&self) -> &[WidgetDescriptor] {
        &self.descriptors
    }

    pub fn get(&self, id: &str) -> Option<&dyn BeskidWidget> {
        self.widgets.get(id).map(|b| b.as_ref())
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Box<dyn BeskidWidget>> {
        self.widgets.get_mut(id)
    }

    pub fn ids(&self) -> &[String] {
        &self.order
    }

    pub fn palette_commands(&self, scope: &ShellScope, layout_edit_active: bool) -> Vec<CommandItem> {
        let mut items = builtin_cli_commands();
        items.extend(builtin_contextual_commands(scope));
        items.extend(layout_editor_commands(layout_edit_active));
        items
    }
}
