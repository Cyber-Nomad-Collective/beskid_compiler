//! Background registry/template work triggered from shell effects and pending flags.

use std::sync::mpsc::Sender;
use std::thread;

use crate::tui::effects::ShellEffect;
use crate::tui::message::ShellMessage;
use crate::tui::panes::{
    fetch_package_details, fetch_packages, install_registry_template, list_installed_templates,
    list_registry_templates, resolve_package_id, search_packages,
};
use crate::tui::shell::state::ShellState;

use super::runtime::RuntimeOp;

pub fn apply_effects(effects: Vec<ShellEffect>, tx: &Sender<RuntimeOp>, state: &mut ShellState) {
    for effect in effects {
        match effect {
            ShellEffect::Redraw | ShellEffect::CloseOverlay | ShellEffect::Quit => {}
            ShellEffect::FetchPckgCatalog => {
                state.pckg.loading = true;
                spawn_pckg_catalog(tx.clone(), state);
            }
            ShellEffect::FetchPckgDetails { package_id } => {
                spawn_pckg_details(tx.clone(), state, package_id);
            }
            ShellEffect::FetchTemplates => {
                state.templates.loading = true;
                spawn_templates_catalog(tx.clone(), state);
            }
            ShellEffect::InstallSelectedTemplate => {
                if let Some(package_id) = state.templates.pending_install.take() {
                    spawn_template_install(tx.clone(), state, package_id);
                }
            }
        }
    }
}

pub fn drain_pending_work(tx: &Sender<RuntimeOp>, state: &mut ShellState) {
    if state.pckg.pending_catalog_refresh {
        state.pckg.pending_catalog_refresh = false;
        state.pckg.loading = true;
        spawn_pckg_catalog(tx.clone(), state);
    }
    if state.templates.pending_catalog_refresh {
        state.templates.pending_catalog_refresh = false;
        state.templates.loading = true;
        spawn_templates_catalog(tx.clone(), state);
    }
    if let Some(package_id) = state.pckg.pending_detail_fetch.take() {
        spawn_pckg_details(tx.clone(), state, package_id);
    }
    if let Some(package_id) = state.templates.pending_install.take() {
        spawn_template_install(tx.clone(), state, package_id);
    }
}

fn spawn_pckg_catalog(tx: Sender<RuntimeOp>, state: &ShellState) {
    let config = state.templates.registry_config.clone();
    let query = state.pckg.search_query.clone();
    thread::spawn(move || {
        let result = if query.trim().is_empty() { fetch_packages(&config) } else { search_packages(&config, &query) };
        let msg = match result {
            Ok(packages) => ShellMessage::PckgCatalogLoaded(packages),
            Err(error) => ShellMessage::PckgCatalogFailed(error.to_string()),
        };
        let _ = tx.send(RuntimeOp::Update(msg));
    });
}

fn spawn_pckg_details(tx: Sender<RuntimeOp>, state: &mut ShellState, package_id: String) {
    state.pckg.detail_loading = true;
    let config = state.templates.registry_config.clone();
    thread::spawn(move || {
        let msg = match fetch_package_details(&config, &package_id) {
            Ok(details) => ShellMessage::PckgDetailsLoaded(Box::new(details)),
            Err(error) => ShellMessage::PckgDetailsFailed(error.to_string()),
        };
        let _ = tx.send(RuntimeOp::Update(msg));
    });
}

fn spawn_templates_catalog(tx: Sender<RuntimeOp>, state: &ShellState) {
    let config = state.templates.registry_config.clone();
    thread::spawn(move || {
        let msg = match (list_installed_templates(), list_registry_templates(&config)) {
            (Ok(installed), Ok(registry)) => ShellMessage::TemplatesLoaded { installed, registry },
            (Err(error), _) | (_, Err(error)) => ShellMessage::TemplatesLoadFailed(error.to_string()),
        };
        let _ = tx.send(RuntimeOp::Update(msg));
    });
}

fn spawn_template_install(tx: Sender<RuntimeOp>, state: &mut ShellState, package_id: String) {
    state.templates.installing = true;
    let config = state.templates.registry_config.clone();
    let resolved = resolve_package_id(&package_id);
    thread::spawn(move || {
        let msg = match install_registry_template(&config, &resolved) {
            Ok(result) => ShellMessage::TemplateInstallDone { short_name: result.short_name, package_id: resolved },
            Err(error) => ShellMessage::TemplateInstallFailed { package_id: resolved, error: error.to_string() },
        };
        let _ = tx.send(RuntimeOp::Update(msg));
    });
}
