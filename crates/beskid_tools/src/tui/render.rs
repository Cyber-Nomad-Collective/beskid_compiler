//! Draw the Beskid shell using ratatui layout and shell overlay chrome.

use ratatui::Frame;

use crate::shell::chrome::ShellChrome;
use crate::shell::hotkeys::ShellHotkeys;
use crate::shell::key_bindings::ShortcutBindings;
use crate::shell::overlay_render::{OverlayRenderContext, render_panel_overlays};
use crate::shell::shortcut_clicks::ShortcutClickTargets;
use crate::tui::layout::{
    PANEL_DETAIL, PANEL_FOOTER, PANEL_LOG, PANEL_STAGE, resolve_shell_layout,
};
use crate::tui::screens::pipeline_compile;
use crate::tui::shell::state::ShellState;

pub fn draw_shell(frame: &mut Frame, state: &mut ShellState) {
    let area = frame.area();
    let rects = resolve_shell_layout(area);
    state.layout_rects = rects;

    let bindings = ShortcutBindings::platform_defaults();
    let scope = crate::shell::scope::ShellScope::resolve(
        &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    );
    ShellChrome::default().render_pinned_top_bar(rects.header, frame, &scope);
    pipeline_compile::render_panel(PANEL_STAGE, rects.stage, frame, state);
    pipeline_compile::render_panel(PANEL_DETAIL, rects.detail, frame, state);
    pipeline_compile::render_panel(PANEL_LOG, rects.log, frame, state);
    pipeline_compile::render_panel(PANEL_FOOTER, rects.footer, frame, state);

    let hotkeys = ShellHotkeys::from_bindings(&bindings);
    let mut click_targets = ShortcutClickTargets::default();
    ShellChrome::default().render_footer(
        rects.chrome,
        frame,
        &hotkeys,
        crate::shell::control_mode::HiControlMode::Normal,
        None,
        false,
        &mut click_targets,
    );

    render_panel_overlays(frame, area, OverlayRenderContext::Pipeline(state));
}
