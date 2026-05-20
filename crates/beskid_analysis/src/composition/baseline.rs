//! Frozen IoC analysis baseline: target-kind fail-closed rules shared by CLI, build, and LSP.
//!
//! - **App / Test**: require exactly one `launch` host; composition errors block lowering when semantic gate is on.
//! - **Lib**: `launch` in a resolved snapshot is an error (`E1711`); hosts and registries may still be analyzed.
//! - **Mod** (`__mod__` target): `host` declarations are rejected (`E1710`).
//! - **Codegen**: `launch` / `with` lower as analysis-validated no-ops until runtime container lowering exists.

use crate::projects::TargetKind;

/// Whether the compile target allows a `launch` statement at runtime lowering time.
#[must_use]
pub const fn launch_lowering_allowed(kind: TargetKind) -> bool {
    matches!(kind, TargetKind::App | TargetKind::Test)
}

/// Whether composition should emit `CompositionLaunchInLibProject` when a launch host is present.
#[must_use]
pub const fn lib_project_rejects_launch(kind: TargetKind) -> bool {
    matches!(kind, TargetKind::Lib)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::TargetKind;

    #[test]
    fn baseline_target_kind_policies_are_frozen() {
        assert!(launch_lowering_allowed(TargetKind::App));
        assert!(launch_lowering_allowed(TargetKind::Test));
        assert!(!launch_lowering_allowed(TargetKind::Lib));
        assert!(lib_project_rejects_launch(TargetKind::Lib));
        assert!(!lib_project_rejects_launch(TargetKind::App));
    }
}
