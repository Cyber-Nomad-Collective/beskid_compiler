//! One-line command context: mode, focus region, active stage, navigation hint.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::model::Model;
use super::super::stage_focus::StageFocus;

pub fn draw_context_bar(frame: &mut Frame, area: Rect, model: &Model, focus: StageFocus) {
    let mode_label = match model.mode {
        super::super::model::Mode::Pipeline => "pipeline",
        super::super::model::Mode::Tests => "tests",
        super::super::model::Mode::Report => "report",
        super::super::model::Mode::Summary => "summary",
    };
    let stage = if model.compile_complete && model.mode == super::super::model::Mode::Pipeline {
        "complete"
    } else if model.pipeline.stage_label.is_empty() {
        "starting"
    } else {
        model.pipeline.stage_label.as_str()
    };
    let mut spans = vec![
        Span::styled(mode_label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" · "),
        Span::styled(focus.title(), Style::default().fg(Color::Yellow)),
        Span::raw(" · "),
        Span::styled(stage, Style::default().fg(Color::White)),
    ];
    if let Some(hint) = model.navigation_hint() {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(hint, Style::default().fg(Color::Green)));
    }
    let widget = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Beskid "),
    );
    frame.render_widget(widget, area);
}
