//! Pipeline TUI — [The Elm Architecture](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/)
//! model / message / update / view.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge};
use tui_logger::TuiWidgetState;
use tui_tree_widget::{Tree, TreeItem, TreeState};
use tui_widget_list::{ListBuilder, ListState, ListView};

use super::logger_panel::{draw_log_panel, init_session_logger};
use super::pipeline_tree::PipelineTree;
use super::test_report::{TestReportSummary, draw_test_report, seed_failure_logs};
use super::test_table::{TestRow, TestRowState};
use super::timer::format_duration;

const FOOTER_HEIGHT: u16 = 5;
const TREE_PANEL_RATIO: u16 = 42;

/// Dual progress-bar state pinned to the layout footer.
#[derive(Debug, Clone)]
pub struct PipelineProgress {
    pub total_pos: u64,
    pub total_len: u64,
    pub total_label: String,
    pub stage_pos: u64,
    pub stage_len: u64,
    pub stage_label: String,
}

impl Default for PipelineProgress {
    fn default() -> Self {
        Self {
            total_pos: 0,
            total_len: 1,
            total_label: "Pipeline".into(),
            stage_pos: 0,
            stage_len: 1,
            stage_label: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pipeline,
    Tests,
    Report,
}

/// Application model: tree, build log buffer, progress, and mode-specific panels.
pub struct Model {
    pub mode: Mode,
    pub pipeline: PipelineProgress,
    pub tree: PipelineTree,
    pub tree_state: TreeState<String>,
    pub test_rows: Vec<TestRow>,
    pub test_title: Option<String>,
    pub test_list_state: ListState,
    pub report_summary: TestReportSummary,
    pub logger_state: TuiWidgetState,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            mode: Mode::Pipeline,
            pipeline: PipelineProgress::default(),
            tree: PipelineTree::default(),
            tree_state: TreeState::default(),
            test_rows: Vec::new(),
            test_title: None,
            test_list_state: ListState::default(),
            report_summary: TestReportSummary::default(),
            logger_state: TuiWidgetState::new(),
        }
    }
}

/// Events that mutate the model (TEA update input).
#[derive(Debug, Clone)]
pub enum Message {
    PhaseStart {
        depth: usize,
        label: String,
    },
    PhaseEnd {
        depth: usize,
        label: String,
        duration: String,
    },
    WorkUnit {
        depth: usize,
        done: u64,
        total: u64,
        label: String,
    },
    SetProgress {
        total_pos: u64,
        total_len: u64,
        total_label: String,
        stage_pos: u64,
        stage_len: u64,
        stage_label: String,
    },
    PushLog(String),
    BeginTests {
        title: String,
        rows: Vec<TestRow>,
    },
    UpdateTestRows(Vec<TestRow>),
    ShowTestReport {
        summary: TestReportSummary,
        title: String,
    },
}

pub fn update(model: &mut Model, msg: Message) {
    match msg {
        Message::PhaseStart { depth, label } => {
            model.tree.phase_start(depth, &label);
            tracing::info!(target: "beskid.tools.pipeline.ui", depth, label = label.as_str(), "phase");
            expand_tree(model);
        }
        Message::PhaseEnd {
            depth,
            label,
            duration,
        } => {
            model.tree.phase_end(depth, &label, &duration);
            tracing::info!(
                target: "beskid.tools.pipeline.ui",
                depth,
                label = label.as_str(),
                duration = duration.as_str(),
                "phase done"
            );
            expand_tree(model);
        }
        Message::WorkUnit {
            depth,
            done,
            total,
            label,
        } => {
            model.tree.work_unit(depth, done, total, &label);
            tracing::trace!(
                target: "beskid.tools.pipeline.ui",
                depth,
                done,
                total,
                label = label.as_str(),
                "work unit"
            );
            expand_tree(model);
        }
        Message::SetProgress {
            total_pos,
            total_len,
            total_label,
            stage_pos,
            stage_len,
            stage_label,
        } => {
            model.pipeline.total_pos = total_pos;
            model.pipeline.total_len = total_len.max(1);
            model.pipeline.total_label = total_label;
            model.pipeline.stage_pos = stage_pos;
            model.pipeline.stage_len = stage_len.max(1);
            model.pipeline.stage_label = stage_label;
        }
        Message::PushLog(line) => {
            tracing::info!(target: "beskid.tools.pipeline", "{line}");
        }
        Message::BeginTests { title, rows } => {
            init_session_logger();
            model.mode = Mode::Tests;
            model.test_title = Some(title);
            model.test_rows = rows;
        }
        Message::UpdateTestRows(rows) => {
            model.test_rows = rows;
        }
        Message::ShowTestReport { summary, title } => {
            init_session_logger();
            seed_failure_logs(&model.test_rows);
            model.report_summary = summary;
            model.test_title = Some(title);
            model.mode = Mode::Report;
        }
    }
}

/// Render the current model (TEA view). Stateful widgets take `&mut model`.
pub fn view(model: &mut Model, frame: &mut Frame) {
    match model.mode {
        Mode::Pipeline => view_pipeline(model, frame),
        Mode::Tests => view_tests(model, frame),
        Mode::Report => view_report(model, frame),
    }
}

fn view_pipeline(model: &mut Model, frame: &mut Frame) {
    let items = model.tree.tree_items().unwrap_or_default();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(TREE_PANEL_RATIO),
            Constraint::Percentage(100 - TREE_PANEL_RATIO),
        ])
        .split(root[0]);

    render_build_tree(frame, body[0], &items, &mut model.tree_state);
    draw_log_panel(frame, body[1], "Build log", &mut model.logger_state);
    render_progress_footer(frame, root[1], &model.pipeline);
}

fn render_build_tree(
    frame: &mut Frame,
    area: Rect,
    items: &[TreeItem<'_, String>],
    tree_state: &mut TreeState<String>,
) {
    if let Ok(tree) = Tree::new(items).map(|tree| {
        tree.block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Build "),
        )
        .highlight_style(Style::default().fg(Color::Cyan))
    }) {
        frame.render_stateful_widget(tree, area, tree_state);
    }
}

fn render_progress_footer(frame: &mut Frame, area: Rect, progress: &PipelineProgress) {
    let footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);

    let stage_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP)
                .title(format!(" {} ", progress.stage_label)),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .percent(percent(progress.stage_pos, progress.stage_len))
        .label(format!("{}/{}", progress.stage_pos, progress.stage_len));

    let total_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .title(format!(" {} ", progress.total_label)),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .percent(percent(progress.total_pos, progress.total_len))
        .label(format!("{}/{}", progress.total_pos, progress.total_len));

    frame.render_widget(stage_gauge, footer[0]);
    frame.render_widget(total_gauge, footer[1]);
}

fn view_tests(model: &mut Model, frame: &mut Frame) {
    let title = model.test_title.as_deref().unwrap_or("Tests");
    let rows = model.test_rows.clone();
    let row_count = rows.len();
    let selected = if row_count > 0 {
        Some(
            rows.iter()
                .position(|row| row.state == TestRowState::Running)
                .or_else(|| {
                    rows.iter()
                        .rposition(|row| row.state != TestRowState::Pending)
                })
                .unwrap_or(0),
        )
    } else {
        None
    };
    let builder = ListBuilder::new(move |context| {
        let row = &rows[context.index];
        let line = format_test_row(row);
        if context.is_selected {
            (line.style(Style::default().bg(Color::DarkGray)), 1)
        } else {
            (line, 1)
        }
    });
    let list = ListView::new(builder, row_count).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} ({row_count}) ")),
    );
    if let Some(index) = selected {
        model.test_list_state.select(Some(index));
    }
    frame.render_stateful_widget(list, frame.area(), &mut model.test_list_state);
}

fn view_report(model: &mut Model, frame: &mut Frame) {
    draw_test_report(
        frame,
        frame.area(),
        model.report_summary,
        model.test_title.as_deref().unwrap_or("Test report"),
        &mut model.logger_state,
    );
}

fn expand_tree(model: &mut Model) {
    if let Ok(items) = model.tree.tree_items() {
        for item in items {
            model.tree_state.open(vec![item.identifier().clone()]);
        }
    }
    for path in model.tree.open_paths() {
        model.tree_state.open(path);
    }
}

fn format_test_row(row: &TestRow) -> Line<'static> {
    let (status, style) = match row.state {
        TestRowState::Pending => ("pending", Style::default().fg(Color::DarkGray)),
        TestRowState::Running => ("running", Style::default().fg(Color::Yellow)),
        TestRowState::Passed => ("pass", Style::default().fg(Color::Green)),
        TestRowState::Failed => ("fail", Style::default().fg(Color::Red)),
        TestRowState::Skipped => ("skip", Style::default().fg(Color::Blue)),
        TestRowState::FilteredOut => ("filt", Style::default().fg(Color::DarkGray)),
    };
    let time = row
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "—".to_owned());
    Line::from(vec![
        Span::styled(format!("{status:<8}"), style),
        Span::raw(format!("{time:>8}  ")),
        Span::raw(row.qualified_name.clone()),
    ])
}

fn percent(done: u64, total: u64) -> u16 {
    let total = total.max(1);
    ((done.saturating_mul(100)) / total).min(100) as u16
}
