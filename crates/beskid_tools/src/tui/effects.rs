//! Side effects returned from shell view updates.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEffect {
    Redraw,
    Quit,
    CloseOverlay,
    FetchPckgCatalog,
    FetchPckgDetails {
        package_id: String,
    },
    FetchTemplates,
    InstallSelectedTemplate,
}
