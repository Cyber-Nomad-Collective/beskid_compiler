//! `--error-format`, accepted by the commands that report compiler diagnostics.

use beskid_tools::diagnostics::{ErrorFormat, set_error_format};
use clap::{Args, ValueEnum};

use super::dev::DevCommand;
use crate::cli::app::Commands;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ErrorFormatArg {
    /// Drawn diagnostics with source excerpts.
    #[default]
    Human,
    /// One JSON object per diagnostic and line on stderr (schema in `docs/diagnostics-json.md`).
    Json,
}

#[derive(Args, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorFormatArgs {
    /// How compiler diagnostics are written to stderr
    #[arg(long = "error-format", value_enum, default_value_t = ErrorFormatArg::Human)]
    pub error_format: ErrorFormatArg,
}

impl ErrorFormatArgs {
    pub fn format(self) -> ErrorFormat {
        match self.error_format {
            ErrorFormatArg::Human => ErrorFormat::Human,
            ErrorFormatArg::Json => ErrorFormat::Json,
        }
    }
}

/// The `--error-format` of `command`, for the commands that accept it.
pub fn error_format_of(command: &Commands) -> Option<ErrorFormatArgs> {
    match command {
        Commands::Check(args) => Some(args.diagnostics),
        Commands::Build(args) => Some(args.diagnostics),
        Commands::Run(args) => Some(args.diagnostics),
        Commands::Test(args) => Some(args.diagnostics),
        Commands::Fmt(args) => Some(args.diagnostics),
        Commands::Dev(args) => match &args.command {
            DevCommand::Build(args) => Some(args.diagnostics),
            _ => None,
        },
        _ => None,
    }
}

/// Apply the invocation's `--error-format` to every diagnostic this process writes.
pub fn apply_error_format(command: &Commands) {
    set_error_format(error_format_of(command).map(ErrorFormatArgs::format).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::app::Cli;
    use clap::Parser;

    fn format_of(arguments: &[&str]) -> Option<ErrorFormat> {
        let cli = Cli::try_parse_from(arguments).expect("arguments parse");
        error_format_of(&cli.command).map(ErrorFormatArgs::format)
    }

    #[test]
    fn diagnostic_commands_accept_error_format_json() {
        for command in ["check", "build", "run", "test", "fmt"] {
            assert_eq!(format_of(&["beskid", command, "--error-format", "json"]), Some(ErrorFormat::Json), "{command}");
            assert_eq!(format_of(&["beskid", command]), Some(ErrorFormat::Human), "{command}");
        }
        assert_eq!(format_of(&["beskid", "dev", "build", "--error-format=json"]), Some(ErrorFormat::Json));
    }

    #[test]
    fn commands_without_compiler_diagnostics_reject_error_format() {
        assert!(Cli::try_parse_from(["beskid", "doctor", "--error-format", "json"]).is_err());
        assert!(Cli::try_parse_from(["beskid", "check", "--error-format", "xml"]).is_err());
    }
}
