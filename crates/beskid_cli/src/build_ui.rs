use std::borrow::Cow;
use std::time::Duration;

use beskid_analysis::services::ResolvedInput;
use indicatif::{ProgressBar, ProgressStyle};

pub struct BuildUx {
    enabled: bool,
    spinner: Option<ProgressBar>,
}

impl BuildUx {
    pub fn start(enabled: bool, resolved: &ResolvedInput) -> Self {
        if !enabled {
            return Self {
                enabled: false,
                spinner: None,
            };
        }

        print_graph(resolved);

        let spinner = ProgressBar::new_spinner();
        let style = ProgressStyle::with_template("{spinner:.blue} {msg}")
            .expect("build spinner template")
            .tick_strings(&["|", "/", "-", "\\", "=", "-", "\\", "/"]);
        spinner.set_style(style);
        spinner.enable_steady_tick(Duration::from_millis(90));

        Self {
            enabled: true,
            spinner: Some(spinner),
        }
    }

    pub fn stage(&self, message: impl Into<Cow<'static, str>>) {
        if let Some(spinner) = &self.spinner {
            spinner.set_message(message.into().into_owned());
        }
    }

    pub fn finish(&self, message: impl Into<Cow<'static, str>>) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(message.into().into_owned());
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

fn print_graph(resolved: &ResolvedInput) {
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return;
    };

    println!("Build graph:");
    println!("  root: {}", plan.project_name);
    if plan.dependency_projects.is_empty() {
        println!("  deps: (none)");
    } else {
        for dependency in &plan.dependency_projects {
            println!(
                "  root -> {} ({})",
                dependency.dependency_name, dependency.project_name
            );
        }
    }

    if plan.has_std_dependency {
        println!("  corelib: project dependency detected");
    } else {
        println!("  corelib: none declared in project graph");
    }
}
