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
mod sources;
mod substitute;
mod symbols;

pub use cache::{
    checksum_dir, find_installed_by_short_name, install_dir_for_identity, install_from_tree,
    list_installed, load_registry_index, read_template_root_from_install, registry_index_path,
    save_registry_index, uninstall_by_short_name, beskid_config_root, InstallSnapshot,
    InstallSource, RegistryIndex, RegistryIndexEntry,
};
pub use error::{TemplateError, TemplateResult};
pub use git::{clone_or_update, git_cache_dir, GitTemplateRef};
pub use instantiate::{
    fixture_manifest, instantiate, InstantiateOptions, InstantiateResult,
};
pub use registry::{extract_bpk_to_dir, verify_template_package};
pub use manifest::{
    load_manifest_from_path, load_manifest_from_template_root, parse_manifest_bytes,
    resolve_package_id, TemplateManifest, TemplateOutputKind, TemplatePostAction, TemplateSource,
    TemplateSymbol, TemplateTags, SymbolType, SHORT_NAME_PACKAGES, TEMPLATE_MANIFEST_REL,
    TEMPLATE_SCHEMA,
};
pub use substitute::{apply_source_name, build_substitution_map, substitute_text};
pub use symbols::{
    collect_symbol_values, parse_symbol_flag, stdin_is_interactive, SymbolCollectOptions,
    SymbolValues,
};
