//! Beskid project template engine (`beskid.template.v1`): manifest, substitution, instantiation.

mod cache;
mod error;
mod forms;
mod git;
mod guids;
mod instantiate;
mod manifest;
mod post_actions;
mod registry;
mod service;
mod sources;
mod substitute;
mod symbols;

pub use cache::{
    InstallSnapshot, InstallSource, RegistryIndex, RegistryIndexEntry, beskid_config_root, checksum_dir,
    find_installed_by_short_name, install_dir_for_identity, install_from_tree, list_installed, load_registry_index,
    read_template_root_from_install, registry_index_path, save_registry_index, uninstall_by_short_name,
};
pub use error::{TemplateError, TemplateResult};
pub use git::{GitTemplateRef, clone_or_update, git_cache_dir};
pub use instantiate::{InstantiateOptions, InstantiateResult, fixture_manifest, instantiate};
pub use manifest::{
    SHORT_NAME_PACKAGES, SymbolType, TEMPLATE_MANIFEST_REL, TEMPLATE_SCHEMA, TemplateManifest, TemplateOutputKind,
    TemplatePostAction, TemplateSource, TemplateSymbol, TemplateTags, load_manifest_from_path,
    load_manifest_from_template_root, parse_manifest_bytes, resolve_package_id,
};
pub use registry::{extract_bpk_to_dir, verify_template_package};
pub use service::{
    InstallTemplateOutput, InstallTemplateRequest, InstalledTemplateRow, InstantiateTemplateRequest,
    ListTemplatesOutput, ListTemplatesRequest, RegistryTemplateRow, TemplateSelector, UninstallTemplateOutput,
    UninstallTemplateRequest, count_selectors, install_template, instantiate_template, list_templates,
    parse_kind_filter, uninstall_template,
};
pub use substitute::{apply_source_name, build_substitution_map, substitute_text};
pub use symbols::{SymbolCollectOptions, SymbolValues, collect_symbol_values, parse_symbol_flag, stdin_is_interactive};
