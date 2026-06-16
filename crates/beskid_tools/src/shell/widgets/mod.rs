//! Built-in shell widgets and registration.

mod analysis;
mod chrome;
mod compile_debug;
mod debug_stub;
mod graph;
mod hi_welcome;
mod log;
mod pckg;
mod pipeline;
mod scope;
mod settings;
mod shortcuts;
mod tests;

use super::registry::WidgetRegistry;

pub use analysis::{draw_analysis_panel, open_analysis, AnalysisWidget};
pub use chrome::ChromeWidget;
pub use compile_debug::{
    draw_compile_debug_panel, open_compile_debug, CompileDebugTab, CompileDebugWidget,
};
pub use debug_stub::DebugFutureWidget;
pub use graph::{draw_graph_deps_panel, GraphCompileWidget, GraphWidget};
pub use hi_welcome::HiWelcomeWidget;
pub use log::LogWidget;
pub use pckg::PckgWidget;
pub use log::LogPanelWidget;
pub use pipeline::{DetailWidget, FooterWidget, HeaderWidget, StageWidget};
pub use pckg::open_pckg;
pub use tests::open_tests;
pub use scope::ScopeWidget;
pub use settings::SettingsWidget;
pub use shortcuts::ShortcutsWidget;
pub use tests::TestsWidget;

/// Register all built-in widgets.
pub fn register_builtins(registry: &mut WidgetRegistry) {
    registry.register(Box::new(ScopeWidget));
    registry.register(Box::new(HiWelcomeWidget));
    registry.register(Box::new(ShortcutsWidget));
    registry.register(Box::new(LogWidget));
    registry.register(Box::new(ChromeWidget));
    registry.register(Box::new(HeaderWidget));
    registry.register(Box::new(StageWidget));
    registry.register(Box::new(DetailWidget));
    registry.register(Box::new(LogPanelWidget));
    registry.register(Box::new(FooterWidget));
    registry.register(Box::new(TestsWidget));
    registry.register(Box::new(PckgWidget));
    registry.register(Box::new(AnalysisWidget));
    registry.register(Box::new(SettingsWidget::default()));
    registry.register(Box::new(GraphWidget));
    registry.register(Box::new(GraphCompileWidget));
    registry.register(Box::new(CompileDebugWidget::default()));
    registry.register(Box::new(DebugFutureWidget));
}
