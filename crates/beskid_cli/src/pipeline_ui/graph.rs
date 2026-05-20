//! Dependency graph rendering for resolved project builds.

use std::io::{self, Write};

use beskid_analysis::services::ResolvedInput;

use super::tui::{write_box_bottom, write_box_line, write_box_top};

/// Write a boxed dependency tree for the compile plan to `out` (typically stderr).
pub fn write_build_graph(resolved: &ResolvedInput, out: &mut dyn Write) -> io::Result<()> {
    if let Some(ws) = &resolved.workspace_summary {
        writeln!(out, "Workspace  {}", ws.workspace_manifest_path.display())?;
        writeln!(out, "Member    {}", ws.selected_member_id)?;
        writeln!(out)?;
    }

    let Some(plan) = resolved.compile_plan.as_ref() else {
        return Ok(());
    };

    write_box_top(out, "Dependency graph")?;
    write_box_line(out, &format!("● {}", plan.project_name))?;

    let dep_count = plan.dependency_projects.len();
    let declares_corelib = plan.dependency_projects.iter().any(|dependency| {
        dependency.project_name == "corelib" || dependency.dependency_name == "corelib"
    });

    for (index, dependency) in plan.dependency_projects.iter().enumerate() {
        let is_last = index + 1 == dep_count && (!plan.has_std_dependency || declares_corelib);
        let branch = if is_last { "└─" } else { "├─" };
        write_box_line(out, &format!("{branch} {}", dependency.project_name))?;
    }

    if plan.has_std_dependency && !declares_corelib {
        write_box_line(out, "└─ corelib (stdlib)")?;
    }

    write_box_bottom(out)?;
    writeln!(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::projects::{
        CompilePlan, ResolvedDependencyProject, Target, TargetKind,
    };
    use beskid_analysis::services::ResolvedInput;
    use std::path::PathBuf;

    #[test]
    fn graph_includes_root_and_dependencies() {
        let resolved = ResolvedInput {
            source_path: PathBuf::from("src/Main.bd"),
            source: String::new(),
            workspace_summary: None,
            prepared_workspace: None,
            compile_plan: Some(CompilePlan {
                project_name: "demo".to_owned(),
                project_root: PathBuf::from("."),
                manifest_path: PathBuf::from("Project.proj"),
                source_root: PathBuf::from("src"),
                target: Target {
                    name: "Demo".to_owned(),
                    kind: TargetKind::App,
                    entry: "Main.bd".to_owned(),
                },
                dependency_projects: vec![ResolvedDependencyProject {
                    dependency_name: "lib".to_owned(),
                    project_name: "my_lib".to_owned(),
                    manifest_path: PathBuf::from("../lib/Project.proj"),
                    project_root: PathBuf::from("../lib"),
                    source_root: PathBuf::from("../lib/src"),
                }],
                unresolved_dependencies: Vec::new(),
                has_std_dependency: true,
            }),
        };

        let mut buf = Vec::new();
        write_build_graph(&resolved, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("Dependency graph"));
        assert!(text.contains("● demo"));
        assert!(text.contains("├─ my_lib"));
        assert!(text.contains("└─ corelib (stdlib)"));
    }

    #[test]
    fn graph_avoids_duplicate_corelib_line() {
        let resolved = ResolvedInput {
            source_path: PathBuf::from("src/Main.bd"),
            source: String::new(),
            workspace_summary: None,
            prepared_workspace: None,
            compile_plan: Some(CompilePlan {
                project_name: "demo".to_owned(),
                project_root: PathBuf::from("."),
                manifest_path: PathBuf::from("Project.proj"),
                source_root: PathBuf::from("src"),
                target: Target {
                    name: "Demo".to_owned(),
                    kind: TargetKind::App,
                    entry: "Main.bd".to_owned(),
                },
                dependency_projects: vec![ResolvedDependencyProject {
                    dependency_name: "corelib".to_owned(),
                    project_name: "corelib".to_owned(),
                    manifest_path: PathBuf::from("corelib/Project.proj"),
                    project_root: PathBuf::from("corelib"),
                    source_root: PathBuf::from("corelib/src"),
                }],
                unresolved_dependencies: Vec::new(),
                has_std_dependency: true,
            }),
        };

        let mut buf = Vec::new();
        write_build_graph(&resolved, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.matches("corelib").count(), 1);
    }
}
