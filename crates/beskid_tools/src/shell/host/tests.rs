use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::HiShellApp;
use crate::shell::layout;
use crate::shell::nav::NavRegistry;
use crate::shell::registry::WidgetRegistry;
use crate::shell::scope::ShellScope;
use crate::shell::settings::ToolSettingsRegistry;
use crate::shell::widgets;
use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent};

fn test_app() -> HiShellApp {
    let scope = ShellScope::User;
    let layout_state = layout::load_for_scope(&scope).expect("layout");
    let mut registry = WidgetRegistry::new();
    widgets::register_builtins(&mut registry);
    let mut nav = NavRegistry::new();
    nav.merge_pages(&layout_state.pages);
    let settings = ToolSettingsRegistry::with_builtins();
    HiShellApp::new(scope, layout_state, registry, nav, settings)
}

#[test]
fn tick_idle_returns_continue() {
    let mut app = test_app();
    assert_eq!(app.handle_shell_event(ShellRealmEvent::Tick), ShellOutcome::Continue);
}

#[test]
fn layout_resolve_error_shows_fallback() {
    let mut app = test_app();
    app.set_frame_area(Rect::new(0, 0, 40, 2));

    let backend = TestBackend::new(40, 2);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.draw_shell(frame)).expect("draw");
    let text: String = terminal.backend().buffer().content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("Layout error"), "expected fallback message in buffer: {text:?}");
}
