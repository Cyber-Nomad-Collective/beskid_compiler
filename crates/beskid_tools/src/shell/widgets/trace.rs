use std::cell::RefCell;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};
use tracing::Level;

use beskid_telemetry::{telemetry_buffer, TelemetryEvent, TelemetrySpan};

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::toolbar_block;
use crate::shell::primitives::Hotkey;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TraceTab {
    #[default]
    Spans,
    Events,
}

impl TraceTab {
    const ALL: [Self; 2] = [Self::Spans, Self::Events];

    fn title(self) -> &'static str {
        match self {
            Self::Spans => "Spans",
            Self::Events => "Events",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Spans => 0,
            Self::Events => 1,
        }
    }
}

struct TraceWidgetState {
    active_tab: TraceTab,
    scroll: u16,
}

impl Default for TraceWidgetState {
    fn default() -> Self {
        Self {
            active_tab: TraceTab::Spans,
            scroll: 0,
        }
    }
}

pub struct TraceWidget {
    state: RefCell<TraceWidgetState>,
}

impl Default for TraceWidget {
    fn default() -> Self {
        Self {
            state: RefCell::new(TraceWidgetState::default()),
        }
    }
}

impl BeskidWidget for TraceWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.trace",
            title: "Tracing",
            icon: "⎇",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        vec![
            Hotkey::new("Tab", "Switch spans / events"),
            Hotkey::new("c", "Clear trace buffer"),
            Hotkey::new("↑/↓", "Scroll"),
        ]
    }

    fn on_input(&mut self, event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        let ShellInput::Key(key) = event else {
            return ShellAction::None;
        };
        let mut state = self.state.borrow_mut();
        match key.code {
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                let idx = state.active_tab.index();
                let prev = if idx == 0 {
                    TraceTab::ALL.len() - 1
                } else {
                    idx - 1
                };
                state.active_tab = TraceTab::ALL[prev];
                state.scroll = 0;
            }
            KeyCode::Tab => {
                let idx = (state.active_tab.index() + 1) % TraceTab::ALL.len();
                state.active_tab = TraceTab::ALL[idx];
                state.scroll = 0;
            }
            KeyCode::Char('c') => {
                telemetry_buffer().clear();
                state.scroll = 0;
            }
            KeyCode::Up => state.scroll = state.scroll.saturating_sub(1),
            KeyCode::Down => state.scroll = state.scroll.saturating_add(1),
            _ => {}
        }
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, _ctx: &mut WidgetContext<'_>) {
        let state = self.state.borrow();
        let snapshot = telemetry_buffer().snapshot();
        draw_trace_panel(
            frame,
            area,
            state.active_tab,
            state.scroll,
            &snapshot.spans,
            &snapshot.events,
        );
    }
}

fn level_style(level: Level) -> Style {
    let color = match level {
        Level::ERROR => Color::Red,
        Level::WARN => Color::Yellow,
        Level::INFO => Color::Cyan,
        Level::DEBUG => Color::DarkGray,
        Level::TRACE => Color::DarkGray,
    };
    Style::default().fg(color)
}

fn format_duration_ms(start: u64, end: Option<u64>) -> String {
    match end {
        Some(end) if end >= start => format!("{}ms", end - start),
        Some(_) => "?".into(),
        None => "active".into(),
    }
}

fn span_line(span: &TelemetrySpan) -> Line<'static> {
    let level = Span::styled(format!("{:5} ", span.level), level_style(span.level));
    let duration = Span::styled(
        format!(" {:>8} ", format_duration_ms(span.started_at_ms, span.ended_at_ms)),
        Style::default().fg(Color::DarkGray),
    );
    let target = Span::styled(
        format!("{} ", span.target),
        Style::default().fg(Color::Blue),
    );
    let name = Span::styled(span.name.clone(), Style::default().add_modifier(Modifier::BOLD));
    Line::from(vec![level, duration, target, name])
}

fn event_line(event: &TelemetryEvent) -> Line<'static> {
    let level = Span::styled(format!("{:5} ", event.level), level_style(event.level));
    let target = Span::styled(
        format!("{} ", event.target),
        Style::default().fg(Color::Blue),
    );
    let message = Span::raw(event.message.clone());
    Line::from(vec![level, target, message])
}

pub fn draw_trace_panel(
    frame: &mut Frame,
    area: Rect,
    tab: TraceTab,
    scroll: u16,
    spans: &[TelemetrySpan],
    events: &[TelemetryEvent],
) {
    let block = toolbar_block("Tracing");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);

    let titles: Vec<Line> = TraceTab::ALL
        .iter()
        .map(|t| Line::from(t.title()))
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default())
        .select(tab.index())
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, chunks[0]);

    let (count_label, items): (String, Vec<ListItem>) = match tab {
        TraceTab::Spans => {
            let mut sorted = spans.to_vec();
            sorted.sort_by_key(|s| s.started_at_ms);
            let items = sorted.iter().map(|s| ListItem::new(span_line(s))).collect();
            (format!("{} spans", sorted.len()), items)
        }
        TraceTab::Events => {
            let mut sorted = events.to_vec();
            sorted.sort_by_key(|e| e.at_ms);
            let items = sorted
                .iter()
                .map(|e| ListItem::new(event_line(e)))
                .collect();
            (format!("{} events", sorted.len()), items)
        }
    };

    let visible: Vec<ListItem> = items
        .into_iter()
        .skip(scroll as usize)
        .collect();

    let list = List::new(visible)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .title(count_label)
                .title_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(list, chunks[1]);
}
