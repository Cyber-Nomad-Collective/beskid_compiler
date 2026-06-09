//! Registry and template data loaders for TUI panes.

pub mod pckg_ops;
pub mod template_ops;

pub use pckg_ops::{fetch_package_details, fetch_packages, search_packages};
pub use template_ops::{
    InstalledTemplateView, RegistryTemplateView, default_registry_config,
    install_registry_template, list_installed_templates, list_registry_templates,
    resolve_package_id,
};
