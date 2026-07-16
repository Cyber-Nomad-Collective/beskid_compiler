use std::cell::RefCell;

use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::widgets::{Paragraph, Tabs};

use crate::pipeline::tui::log_tabs::LogTab;
use crate::pipeline::tui::stage_focus::StageFocus;
use crate::pipeline::tui::widgets::{draw_log_panel, draw_pipeline_tree, draw_progress_footer};
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::toolbar_block;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};
use crate::tui::shell::focus::OverlayKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompileDebugTab {
    #[default]
    Timeline,
    Incremental,
    Traces,
}

impl CompileDebugTab {
    const ALL: [Self; 3] = [Self::Timeline, Self::Incremental, Self::Traces];

    fn title(self) -> &'static str {
        match self {
            Self::Timeline => "Timeline",
            Self::Incremental => "Incremental",
            Self::Traces => "Traces",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Timeline => 0,
            Self::Incremental => 1,
            Self::Traces => 2,
        }
    }
}

struct CompileDebugWidgetState {
    active_tab: CompileDebugTab,
}

impl Default for CompileDebugWidgetState {
    fn default() -> Self {
        Self {
            active_tab: CompileDebugTab::Timeline,
        }
    }
}

pub struct CompileDebugWidget {
    state: RefCell<CompileDebugWidgetState>,
}

impl Default for CompileDebugWidget {
    fn default() -> Self {
        Self {
            state: RefCell::new(CompileDebugWidgetState::default()),
        }
    }
}

impl BeskidWidget for CompileDebugWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "compile.debugger",
            title: "Compile debugger",
            icon: "▣",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let tab = self.state.borrow().active_tab;
        draw_compile_debug_panel(frame, area, Some(ctx.scope), ctx.shell_state, tab);
    }
}

pub fn draw_compile_debug_panel(
    frame: &mut Frame,
    area: Rect,
    scope: Option<&ShellScope>,
    state: &mut crate::tui::shell::state::ShellState,
    active_tab: CompileDebugTab,
) {
    let [tabs_area, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);

    if scope.is_some_and(|s| s.is_user()) && !state.pipeline_active() {
        let tabs = Tabs::new(vec!["Timeline", "Incremental", "Traces"])
            .block(toolbar_block("Compile debugger"))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(tabs, tabs_area);
        frame.render_widget(
            Paragraph::new(ShellScope::no_project_lines(
                &crate::shell::key_bindings::ShortcutBindings::platform_defaults().palette_hint(),
            )),
            body,
        );
        return;
    }

    let titles: Vec<&str> = CompileDebugTab::ALL.iter().map(|tab| tab.title()).collect();
    let tabs = Tabs::new(titles)
        .block(toolbar_block("Compile debugger"))
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .select(active_tab.index())
        .divider(symbols::DOT)
        .padding(" ", " ");
    frame.render_widget(tabs, tabs_area);

    match active_tab {
        CompileDebugTab::Timeline => draw_timeline_tab(frame, body, state),
        CompileDebugTab::Incremental => draw_incremental_tab(frame, body, state),
        CompileDebugTab::Traces => draw_traces_tab(frame, body, state),
    }
}

fn draw_timeline_tab(
    frame: &mut Frame,
    area: Rect,
    state: &mut crate::tui::shell::state::ShellState,
) {
    let [progress, tree] =
        Layout::vertical([Constraint::Length(5), Constraint::Min(4)]).areas(area);
    draw_progress_footer(frame, progress, &state.pipeline);
    let focus = StageFocus::from_shell_state(state);
    draw_pipeline_tree(
        frame,
        tree,
        &state.tree_nodes,
        &mut state.tree_state,
        focus.title(),
    );
}

fn draw_incremental_tab(
    frame: &mut Frame,
    area: Rect,
    state: &mut crate::tui::shell::state::ShellState,
) {
    draw_log_panel(
        frame,
        area,
        LogTab::Incremental.scroll_hint(),
        state.log_states.state_mut(LogTab::Incremental),
    );
}

fn draw_traces_tab(
    frame: &mut Frame,
    area: Rect,
    state: &mut crate::tui::shell::state::ShellState,
) {
    draw_log_panel(
        frame,
        area,
        LogTab::Traces.scroll_hint(),
        state.log_states.state_mut(LogTab::Traces),
    );
}

pub fn open_compile_debug(ctx: &mut WidgetContext<'_>) {
    ctx.shell_state
        .set_overlay_visible(OverlayKind::CompileDebug, true);
    ctx.shell_state.focus_overlay(OverlayKind::CompileDebug);
}
