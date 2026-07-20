//! State for pckg browser and new-project template panes.

use beskid_pckg::models::{PackageDetailsResponse, PackageSummaryResponse};
use ratatui::widgets::ListState;

use crate::registry::RegistryConnectConfig;
use crate::tui::panes::{InstalledTemplateView, RegistryTemplateView};

/// Shell operating mode: compile pipeline vs new-project wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellMode {
    #[default]
    Compile,
    ProjectWizard,
    Hi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateListTab {
    #[default]
    Installed,
    Registry,
}

#[derive(Debug, Clone, Default)]
pub struct PckgPaneState {
    pub packages: Vec<PackageSummaryResponse>,
    pub list_state: ListState,
    pub search_query: String,
    pub detail: Option<PackageDetailsResponse>,
    pub loading: bool,
    pub detail_loading: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    pub catalog_loaded: bool,
    pub pending_detail_fetch: Option<String>,
    pub pending_catalog_refresh: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TemplatesPaneState {
    pub tab: TemplateListTab,
    pub installed: Vec<InstalledTemplateView>,
    pub registry: Vec<RegistryTemplateView>,
    pub list_state: ListState,
    pub loading: bool,
    pub installing: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    pub catalog_loaded: bool,
    pub registry_config: RegistryConnectConfig,
    pub pending_install: Option<String>,
    pub pending_catalog_refresh: bool,
}

impl PckgPaneState {
    pub fn selected_package_id(&self) -> Option<&str> {
        self.list_state
            .selected()
            .and_then(|index| self.packages.get(index))
            .map(|pkg| pkg.name.as_str())
    }
}

impl TemplatesPaneState {
    pub fn active_rows(&self) -> usize {
        match self.tab {
            TemplateListTab::Installed => self.installed.len(),
            TemplateListTab::Registry => self.registry.len(),
        }
    }

    pub fn selected_package_id(&self) -> Option<String> {
        let index = self.list_state.selected()?;
        match self.tab {
            TemplateListTab::Installed => self
                .installed
                .get(index)
                .and_then(|row| row.package_id.clone()),
            TemplateListTab::Registry => self.registry.get(index).map(|row| row.package_id.clone()),
        }
    }

    pub fn selected_short_name(&self) -> Option<&str> {
        let index = self.list_state.selected()?;
        match self.tab {
            TemplateListTab::Installed => {
                self.installed.get(index).map(|row| row.short_name.as_str())
            }
            TemplateListTab::Registry => None,
        }
    }
}
