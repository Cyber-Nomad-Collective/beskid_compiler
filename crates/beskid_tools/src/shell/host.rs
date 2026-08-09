//! Shell host: `beskid hi` entry and tuirealm runtime.

mod actions;
mod app;
mod event_route;
mod input_route;
mod layers;
mod rendering;

#[cfg(test)]
mod tests;

pub use app::HiShellApp;

use std::env;
use std::io::{self, IsTerminal, stderr};

use super::nav::{NavRegistrar, NavRegistry};
use super::registry::WidgetRegistry;
use super::scope::ShellScope;
use super::settings::{ToolSettingsRegistrar, ToolSettingsRegistry};
use super::widgets;
use super::{catalog, chrome, input, layout};

pub type WidgetRegistrar = fn(&mut WidgetRegistry);

pub struct ShellHost;

impl ShellHost {
    pub fn interactive_available(plain: bool) -> bool {
        !plain && !no_color_requested() && stderr().is_terminal()
    }

    pub fn run_hi_blocking(
        scope: ShellScope,
        plain: bool,
        widget_registrars: &[WidgetRegistrar],
        nav_registrars: &[NavRegistrar],
        settings_registrars: &[ToolSettingsRegistrar],
    ) -> io::Result<()> {
        if !Self::interactive_available(plain) {
            eprintln!("beskid hi: terminal UI requires an interactive stderr TTY");
            return Ok(());
        }
        let layout = layout::load_for_scope(&scope).map_err(io::Error::other)?;
        let mut registry = WidgetRegistry::new();
        widgets::register_builtins(&mut registry);
        for register in widget_registrars {
            register(&mut registry);
        }
        let mut nav = NavRegistry::new();
        nav.merge_pages(&layout.pages);
        for register in nav_registrars {
            register(&mut nav);
        }
        let mut settings = ToolSettingsRegistry::with_builtins();
        for register in settings_registrars {
            register(&mut settings);
        }
        let app = HiShellApp::new(scope, layout, registry, nav, settings);
        crate::tui::realm::run_hi(app)
    }
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some()
}
