//! The target a command selects when the user gives no `--target`.
//!
//! `beskid build` and `beskid run` never select a `Test` target: `run` selects the App target and
//! `build` selects the App target, or the Lib target when the manifest has no App target. `beskid
//! test` selects every Test target in manifest order, or the Lib target when there is none. Two
//! candidates of the same kind are an error that names them: there is no fallback.

use std::fmt;

use super::model::{Target, TargetKind};

/// The command that selects a default target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultTargetCommand {
    Build,
    Run,
    Test,
}

impl DefaultTargetCommand {
    fn name(self) -> &'static str {
        match self {
            Self::Build => "beskid build",
            Self::Run => "beskid run",
            Self::Test => "beskid test",
        }
    }
}

/// No target, or more than one target, matches the default rule of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultTargetError {
    pub command: DefaultTargetCommand,
    /// The kind that has more than one target, or `None` when no target matches.
    pub ambiguous_kind: Option<TargetKind>,
    /// `(name, kind)` of the competing targets, or of every target when no target matches.
    pub targets: Vec<(String, TargetKind)>,
}

impl fmt::Display for DefaultTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = self.command.name();
        let listed =
            self.targets.iter().map(|(name, kind)| format!("`{name}` ({kind:?})")).collect::<Vec<_>>().join(", ");
        match self.ambiguous_kind {
            Some(kind) => write!(
                f,
                "`{command}` found {} {kind:?} targets ({listed}) and cannot choose one; pass `--target <name>`",
                self.targets.len()
            ),
            None => {
                let wanted = match self.command {
                    DefaultTargetCommand::Build => "an App or Lib target (a Test target is built only with `--target`)",
                    DefaultTargetCommand::Run => "an App target",
                    DefaultTargetCommand::Test => "a Test or Lib target",
                };
                let declared = if listed.is_empty() { "no targets".to_string() } else { listed };
                write!(f, "`{command}` needs {wanted}; the manifest declares {declared}; pass `--target <name>`")
            }
        }
    }
}

impl std::error::Error for DefaultTargetError {}

/// The one target `beskid build` or `beskid run` uses without `--target`.
///
/// `run` takes the App target. `build` takes the App target, or the Lib target when there is no
/// App target. A Test target is never selected. More than one candidate of the selected kind, or
/// no candidate, is an error that lists the targets.
pub fn select_default_target(targets: &[Target], command: DefaultTargetCommand) -> Result<&Target, DefaultTargetError> {
    let kinds: &[TargetKind] = match command {
        DefaultTargetCommand::Build => &[TargetKind::App, TargetKind::Lib],
        DefaultTargetCommand::Run => &[TargetKind::App],
        // `beskid test` runs several targets; see `select_default_test_targets`.
        DefaultTargetCommand::Test => &[TargetKind::Test, TargetKind::Lib],
    };
    for &kind in kinds {
        let candidates = targets.iter().filter(|target| target.kind == kind).collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => continue,
            [only] => return Ok(only),
            _ => return Err(ambiguous(command, kind, &candidates)),
        }
    }
    Err(DefaultTargetError { command, ambiguous_kind: None, targets: described(targets.iter()) })
}

/// The targets `beskid test` runs without `--target`, in manifest order.
///
/// Every Test target runs. Without a Test target the one Lib target runs; more than one Lib target
/// is an error. An empty result means the manifest has neither (for example an App alone), so the
/// run has no tests.
pub fn select_default_test_targets(targets: &[Target]) -> Result<Vec<&Target>, DefaultTargetError> {
    let tests = targets.iter().filter(|target| target.kind == TargetKind::Test).collect::<Vec<_>>();
    if !tests.is_empty() {
        return Ok(tests);
    }
    let libs = targets.iter().filter(|target| target.kind == TargetKind::Lib).collect::<Vec<_>>();
    if libs.len() > 1 {
        return Err(ambiguous(DefaultTargetCommand::Test, TargetKind::Lib, &libs));
    }
    Ok(libs)
}

fn ambiguous(command: DefaultTargetCommand, kind: TargetKind, candidates: &[&Target]) -> DefaultTargetError {
    DefaultTargetError { command, ambiguous_kind: Some(kind), targets: described(candidates.iter().copied()) }
}

fn described<'a>(targets: impl Iterator<Item = &'a Target>) -> Vec<(String, TargetKind)> {
    targets.map(|target| (target.name.clone(), target.kind)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, kind: TargetKind) -> Target {
        Target { name: name.to_string(), kind, entry: Some(format!("{name}.bd")) }
    }

    fn names(targets: Vec<&Target>) -> Vec<&str> {
        targets.into_iter().map(|target| target.name.as_str()).collect()
    }

    #[test]
    fn build_selects_the_lib_target_and_never_a_test_target() {
        let targets = [target("lib", TargetKind::Lib), target("tests", TargetKind::Test)];
        assert_eq!(select_default_target(&targets, DefaultTargetCommand::Build).unwrap().name, "lib");
        let targets = [target("tests", TargetKind::Test), target("lib", TargetKind::Lib)];
        assert_eq!(select_default_target(&targets, DefaultTargetCommand::Build).unwrap().name, "lib");
    }

    #[test]
    fn build_and_run_select_the_app_target_over_lib_and_test() {
        let targets =
            [target("lib", TargetKind::Lib), target("tests", TargetKind::Test), target("app", TargetKind::App)];
        assert_eq!(select_default_target(&targets, DefaultTargetCommand::Build).unwrap().name, "app");
        assert_eq!(select_default_target(&targets, DefaultTargetCommand::Run).unwrap().name, "app");
    }

    #[test]
    fn two_targets_of_the_selected_kind_are_an_error_that_lists_them() {
        let targets = [target("one", TargetKind::App), target("two", TargetKind::App), target("lib", TargetKind::Lib)];
        let error = select_default_target(&targets, DefaultTargetCommand::Run).unwrap_err();
        assert_eq!(
            error.to_string(),
            "`beskid run` found 2 App targets (`one` (App), `two` (App)) and cannot choose one; pass `--target <name>`"
        );
        assert!(select_default_target(&targets, DefaultTargetCommand::Build).is_err());
        let libs = [target("a", TargetKind::Lib), target("b", TargetKind::Lib)];
        let error = select_default_target(&libs, DefaultTargetCommand::Build).unwrap_err();
        assert_eq!(error.ambiguous_kind, Some(TargetKind::Lib));
    }

    #[test]
    fn a_test_only_manifest_has_no_build_target_and_run_needs_an_app() {
        let targets = [target("tests", TargetKind::Test)];
        let error = select_default_target(&targets, DefaultTargetCommand::Build).unwrap_err();
        assert_eq!(error.ambiguous_kind, None);
        assert!(error.to_string().contains("`tests` (Test)"), "{error}");
        let targets = [target("lib", TargetKind::Lib)];
        let error = select_default_target(&targets, DefaultTargetCommand::Run).unwrap_err();
        assert_eq!(
            error.to_string(),
            "`beskid run` needs an App target; the manifest declares `lib` (Lib); pass `--target <name>`"
        );
    }

    #[test]
    fn test_selects_every_test_target_in_manifest_order() {
        let targets = [
            target("lib", TargetKind::Lib),
            target("unit", TargetKind::Test),
            target("app", TargetKind::App),
            target("integration", TargetKind::Test),
        ];
        assert_eq!(names(select_default_test_targets(&targets).unwrap()), ["unit", "integration"]);
    }

    #[test]
    fn test_without_a_test_target_selects_the_lib_target() {
        let targets = [target("app", TargetKind::App), target("lib", TargetKind::Lib)];
        assert_eq!(names(select_default_test_targets(&targets).unwrap()), ["lib"]);
        assert!(select_default_test_targets(&[target("app", TargetKind::App)]).unwrap().is_empty());
        let libs = [target("a", TargetKind::Lib), target("b", TargetKind::Lib)];
        assert_eq!(select_default_test_targets(&libs).unwrap_err().ambiguous_kind, Some(TargetKind::Lib));
    }
}
