#[cfg(test)]
mod tests {
    use super::super::init::initialize_result;
    use crate::commands::PROJECT_EXPLORER_COMMANDS;
    use tower_lsp_server::ls_types::OneOf;

    #[test]
    fn initialize_advertises_project_explorer_commands() {
        let init = initialize_result();
        let provider = init.capabilities.execute_command_provider.expect("execute commands");
        let advertised = provider.commands;
        for expected in PROJECT_EXPLORER_COMMANDS {
            assert!(advertised.iter().any(|c| c == *expected), "missing advertised command {expected}");
        }
    }

    #[test]
    fn initialize_enables_workspace_folders() {
        let init = initialize_result();
        let workspace = init.capabilities.workspace.expect("workspace caps");
        let folders = workspace.workspace_folders.expect("folder support");
        assert_eq!(folders.supported, Some(true));
        assert!(matches!(folders.change_notifications, Some(OneOf::Left(true))));
    }

    #[test]
    fn initialize_triggers_bsol_annotation_completion() {
        let init = initialize_result();
        let provider = init.capabilities.completion_provider.expect("completion provider");
        let triggers = provider.trigger_characters.expect("completion trigger characters");

        assert!(triggers.iter().any(|trigger| trigger == "@"));
    }
}
