//! Draw the Beskid shell using ratatui layout and ratkit overlay chrome.

use ratatui::Frame;
use ratkit::widgets::HotkeyItem;

use crate::tui::layout::{
    overlay_rect_for, resolve_shell_layout, OVERLAY_PCKG, OVERLAY_SUMMARY, OVERLAY_TEMPLATES,
    OVERLAY_TESTS, PANEL_DETAIL, PANEL_FOOTER, PANEL_HEADER, PANEL_LOG, PANEL_STAGE,
};
use crate::shell::chrome::ShellChrome;
use crate::shell::hotkeys::ShellHotkeys;
use crate::tui::overlay_chrome::{draw_backdrop, hotkey, render_overlay_panel};
use crate::tui::screens::{
    pckg_overlay, pipeline_compile, summary_overlay, templates_overlay, tests_overlay,
};
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::state::ShellState;

pub fn draw_shell(frame: &mut Frame, state: &mut ShellState) {
    let area = frame.area();
    let rects = resolve_shell_layout(area);
    state.layout_rects = rects;

    pipeline_compile::render_panel(PANEL_HEADER, rects.header, frame, state);
    pipeline_compile::render_panel(PANEL_STAGE, rects.stage, frame, state);
    pipeline_compile::render_panel(PANEL_DETAIL, rects.detail, frame, state);
    pipeline_compile::render_panel(PANEL_LOG, rects.log, frame, state);
    pipeline_compile::render_panel(PANEL_FOOTER, rects.footer, frame, state);

    let hotkeys = ShellHotkeys::default();
    ShellChrome::default().render_footer(
        rects.chrome,
        frame,
        &hotkeys,
        None,
    );

    let any_overlay = state.overlay_visible(OverlayKind::Tests)
        || state.overlay_visible(OverlayKind::Summary)
        || state.overlay_visible(OverlayKind::Pckg)
        || state.overlay_visible(OverlayKind::Templates);
    if any_overlay {
        draw_backdrop(frame, area);
    }

    if state.overlay_visible(OverlayKind::Tests) {
        let overlay = overlay_rect_for(OVERLAY_TESTS, area);
        state.layout_rects.tests_overlay = Some(overlay);
        let title = tests_title(state);
        render_overlay_panel(
            frame,
            overlay,
            &title,
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
    if state.shell_mode == crate::tui::shell::pane_state::ShellMode::ProjectWizard {
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
    if state.shell_mode == crate::tui::shell::pane_state::ShellMode::ProjectWizard {
        keys.push(hotkey("Enter", "scaffold"));
    }
    keys
}
