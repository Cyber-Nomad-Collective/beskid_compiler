//! Shared modal overlay rendering for hi shell and pipeline compile TUI.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::context::WidgetContext;
use crate::shell::layout::overlays::{
    overlay_rect_for, OVERLAY_ANALYSIS, OVERLAY_COMPILE_DEBUG, OVERLAY_GRAPH, OVERLAY_PCKG,
    OVERLAY_SETTINGS, OVERLAY_SUMMARY, OVERLAY_TEMPLATES, OVERLAY_TESTS,
};
use crate::shell::primitives::HotkeyItem;
use crate::shell::registry::WidgetRegistry;
use crate::tui::overlay_chrome::{draw_backdrop, hotkey, render_overlay_panel};
use crate::tui::screens::{
    analysis_overlay, compile_debug_overlay, graph_overlay, pckg_overlay, settings_overlay,
    summary_overlay, templates_overlay, tests_overlay,
};
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::pane_state::ShellMode;
use crate::tui::shell::state::ShellState;

/// Widget-backed overlays (graph, analysis, settings) need hi shell context.
pub struct HiOverlayWidgets<'a> {
    pub ctx: &'a mut WidgetContext<'a>,
    pub registry: &'a WidgetRegistry,
}

/// Pipeline compile TUI state or hi shell widget context.
pub enum OverlayRenderContext<'a> {
    Pipeline(&'a mut ShellState),
    Hi(HiOverlayWidgets<'a>),
}

pub fn any_panel_overlay_visible(state: &ShellState) -> bool {
    OverlayKind::ALL.iter().any(|kind| state.overlay_visible(*kind))
}

/// Draw backdrop and all visible panel overlays with ratkit Pane chrome.
pub fn render_panel_overlays(
    frame: &mut Frame,
    area: Rect,
    context: OverlayRenderContext<'_>,
) {
    match context {
        OverlayRenderContext::Pipeline(state) => {
            render_pipeline_overlays(frame, area, state);
        }
        OverlayRenderContext::Hi(widgets) => {
            render_hi_overlays(frame, area, widgets);
        }
    }
}

fn render_pipeline_overlays(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    if !any_panel_overlay_visible(state) {
        return;
    }
    draw_backdrop(frame, area);
    render_state_overlays(frame, area, state);
}

fn render_hi_overlays(frame: &mut Frame, area: Rect, widgets: HiOverlayWidgets<'_>) {
    if !any_panel_overlay_visible(widgets.ctx.shell_state) {
        return;
    }
    draw_backdrop(frame, area);
    render_state_overlays(frame, area, widgets.ctx.shell_state);
    render_widget_overlays(frame, area, widgets);
}

fn render_widget_overlays(frame: &mut Frame, area: Rect, widgets: HiOverlayWidgets<'_>) {
    if widgets.ctx.shell_state.overlay_visible(OverlayKind::Graph) {
        let overlay = overlay_rect_for(OVERLAY_GRAPH, area);
        widgets.ctx.shell_state.layout_rects.graph_overlay = Some(overlay);
        let ctx = &mut *widgets.ctx;
        render_overlay_panel(
            frame,
            overlay,
            "Dependency graph",
            &simple_overlay_hotkeys(),
            |body, frame| graph_overlay::render(body, frame, ctx),
        );
    }
    if widgets.ctx.shell_state.overlay_visible(OverlayKind::Settings) {
        let overlay = overlay_rect_for(OVERLAY_SETTINGS, area);
        widgets.ctx.shell_state.layout_rects.settings_overlay = Some(overlay);
        let ctx = &mut *widgets.ctx;
        let registry = widgets.registry;
        render_overlay_panel(
            frame,
            overlay,
            "Settings",
            &simple_overlay_hotkeys(),
            |body, frame| settings_overlay::render(body, frame, ctx, registry),
        );
    }
    if widgets.ctx.shell_state.overlay_visible(OverlayKind::Analysis) {
        let overlay = overlay_rect_for(OVERLAY_ANALYSIS, area);
        widgets.ctx.shell_state.layout_rects.analysis_overlay = Some(overlay);
        let ctx = &mut *widgets.ctx;
        render_overlay_panel(
            frame,
            overlay,
            "Analysis",
            &simple_overlay_hotkeys(),
            |body, frame| analysis_overlay::render(body, frame, ctx),
        );
    }
}

fn render_state_overlays(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    if state.overlay_visible(OverlayKind::Tests) {
        let overlay = overlay_rect_for(OVERLAY_TESTS, area);
        state.layout_rects.tests_overlay = Some(overlay);
        render_overlay_panel(
            frame,
            overlay,
            &tests_title(state),
            &tests_hotkeys(state),
            |body, frame| tests_overlay::render(body, frame, state),
        );
    }
    if state.overlay_visible(OverlayKind::Summary) {
        let overlay = overlay_rect_for(OVERLAY_SUMMARY, area);
        state.layout_rects.summary_overlay = Some(overlay);
        render_overlay_panel(
            frame,
            overlay,
            "Run summary",
            &summary_hotkeys(),
            |body, frame| summary_overlay::render(body, frame, state),
        );
    }
    if state.overlay_visible(OverlayKind::Pckg) {
        let overlay = overlay_rect_for(OVERLAY_PCKG, area);
        state.layout_rects.pckg_overlay = Some(overlay);
        render_overlay_panel(
            frame,
            overlay,
            "pckg registry",
            &pckg_hotkeys(),
            |body, frame| pckg_overlay::render(body, frame, state),
        );
    }
    if state.overlay_visible(OverlayKind::Templates) {
        let overlay = overlay_rect_for(OVERLAY_TEMPLATES, area);
        state.layout_rects.templates_overlay = Some(overlay);
        render_overlay_panel(
            frame,
            overlay,
            templates_title(state),
            &templates_hotkeys(state),
            |body, frame| templates_overlay::render(body, frame, state),
        );
    }
    if state.overlay_visible(OverlayKind::CompileDebug) {
        let overlay = overlay_rect_for(OVERLAY_COMPILE_DEBUG, area);
        state.layout_rects.compile_debug_overlay = Some(overlay);
        render_overlay_panel(
            frame,
            overlay,
            "Compile debugger",
            &simple_overlay_hotkeys(),
            |body, frame| compile_debug_overlay::render(body, frame, state),
        );
    }
}

pub fn simple_overlay_hotkeys() -> Vec<HotkeyItem> {
    vec![hotkey("q", "close")]
}

fn tests_title(state: &ShellState) -> String {
    state
        .test_title
        .as_deref()
        .map(|title| format!("{title} ({})", state.test_rows.len()))
        .unwrap_or_else(|| format!("Tests ({})", state.test_rows.len()))
}

fn tests_hotkeys(state: &ShellState) -> Vec<HotkeyItem> {
    let mut keys = vec![
        hotkey("q", "close"),
        hotkey("Tab", "list/code"),
        hotkey("↑↓", "navigate"),
    ];
    if state.navigation_hint().is_some() {
        keys.push(hotkey("Space", "summary"));
    }
    keys
}

fn summary_hotkeys() -> Vec<HotkeyItem> {
    vec![
        hotkey("Space", "exit"),
        hotkey("q", "close"),
        hotkey("Tab", "list/code"),
        hotkey("↑↓", "failed tests"),
    ]
}

fn pckg_hotkeys() -> Vec<HotkeyItem> {
    vec![
        hotkey("q", "close"),
        hotkey("r", "refresh"),
        hotkey("Tab", "list/readme"),
        hotkey("↑↓", "packages"),
    ]
}

fn templates_title(state: &ShellState) -> &'static str {
    if state.shell_mode == ShellMode::ProjectWizard {
        "New project"
    } else {
        "Templates"
    }
}

fn templates_hotkeys(state: &ShellState) -> Vec<HotkeyItem> {
    let mut keys = vec![
        hotkey("q", "close"),
        hotkey("Tab", "installed/registry"),
        hotkey("i", "install"),
        hotkey("r", "refresh"),
        hotkey("↑↓", "select"),
    ];
    if state.shell_mode == ShellMode::ProjectWizard {
        keys.push(hotkey("Enter", "scaffold"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::tui::shell::focus::OverlayKind;

    #[test]
    fn compile_debug_overlay_renders_pane_title() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut state = ShellState::default();
        state.set_overlay_visible(OverlayKind::CompileDebug, true);

        terminal
            .draw(|frame| {
                render_panel_overlays(
                    frame,
                    frame.area(),
                    OverlayRenderContext::Pipeline(&mut state),
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let text: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Compile debugger"));
    }

    #[test]
    fn graph_overlay_size_matches_compile_debug() {
        use crate::shell::layout::overlays::{
            graph_overlay_size, overlay_rect_for, OVERLAY_COMPILE_DEBUG, OVERLAY_GRAPH,
        };
        let area = ratatui::layout::Rect::new(0, 0, 100, 30);
        let graph = overlay_rect_for(OVERLAY_GRAPH, area);
        let debug = overlay_rect_for(OVERLAY_COMPILE_DEBUG, area);
        assert_eq!(graph.width, debug.width);
        assert_eq!(graph.height, debug.height);
        let (w, h) = graph_overlay_size();
        assert_eq!(w, 80);
        assert_eq!(h, 24);
    }

    #[test]
    fn graph_overlay_renders_pane_title() {
        use std::path::PathBuf;

        use crate::shell::context::WidgetContext;
        use crate::shell::key_bindings::ShortcutBindings;
        use crate::shell::layout::{parse_v2, EMBEDDED_HI_V2};
        use crate::shell::palette::CommandPaletteState;
        use crate::shell::registry::WidgetRegistry;
        use crate::shell::scope::ShellScope;
        use crate::shell::shortcut_clicks::ShortcutClickTargets;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut shell_state = ShellState::default();
        shell_state.set_overlay_visible(OverlayKind::Graph, true);
        let scope = ShellScope::User;
        let layout = parse_v2(EMBEDDED_HI_V2).expect("board");
        let mut palette = CommandPaletteState::default();
        let beskid_exe = PathBuf::from("beskid");
        let mut key_bindings = ShortcutBindings::platform_defaults();
        let mut shortcut_clicks = ShortcutClickTargets::default();
        let mut pending_shortcut_rebind = None;
        let registry = WidgetRegistry::default();
        let mut ctx = WidgetContext::new(
            &scope,
            &layout,
            &mut shell_state,
            &mut palette,
            "",
            &beskid_exe,
            &mut key_bindings,
            &mut shortcut_clicks,
            &mut pending_shortcut_rebind,
        );

        terminal
            .draw(|frame| {
                render_panel_overlays(
                    frame,
                    frame.area(),
                    OverlayRenderContext::Hi(HiOverlayWidgets {
                        ctx: &mut ctx,
                        registry: &registry,
                    }),
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let text: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Dependency graph"));
    }
}
