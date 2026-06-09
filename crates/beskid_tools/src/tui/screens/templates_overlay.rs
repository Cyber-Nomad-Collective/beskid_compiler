//! New-project template picker: installed + registry templates with download.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};

use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::input;
use crate::tui::shell::pane_state::{ShellMode, TemplateListTab};
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
        let mut effects = Vec::new();
        match msg {
            ShellMessage::SetOverlayVisible {
                kind: OverlayKind::Templates,
                visible: true,
            }
            | ShellMessage::EnterProjectWizard => {
                state.shell_mode = ShellMode::ProjectWizard;
                state.set_overlay_visible(OverlayKind::Templates, true);
                state.focus_overlay(OverlayKind::Templates);
                if !state.templates.catalog_loaded && !state.templates.loading {
                    effects.push(ShellEffect::FetchTemplates);
                }
            }
            ShellMessage::TemplatesLoaded { installed, registry } => {
                state.templates.loading = false;
                state.templates.catalog_loaded = true;
                state.templates.error = None;
                state.templates.installed.clone_from(installed);
                state.templates.registry.clone_from(registry);
                if state.templates.active_rows() > 0
                    && state.templates.list_state.selected().is_none()
                {
                    state.templates.list_state.select(Some(0));
                }
                state.sync_template_detail_viewer();
            }
            ShellMessage::TemplatesLoadFailed(error) => {
                state.templates.loading = false;
                state.templates.error = Some(error.clone());
            }
            ShellMessage::TemplateInstallDone { short_name, package_id } => {
                state.templates.installing = false;
                state.templates.status = Some(format!(
                    "Installed `{short_name}` from `{package_id}`"
                ));
                effects.push(ShellEffect::FetchTemplates);
            }
            ShellMessage::TemplateInstallFailed { package_id, error } => {
                state.templates.installing = false;
                state.templates.status =
                    Some(format!("Install failed for `{package_id}`: {error}"));
            }
            _ => {}
        }
    effects
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_templates_overlay_input(event, state)
}

pub fn render(area: Rect, frame: &mut Frame, state: &mut ShellState) {
    let [top, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(6)]).areas(area);
    let [list_area, detail_area] = Layout::horizontal([
        Constraint::Percentage(42),
        Constraint::Percentage(58),
    ])
    .areas(body);

    draw_tabs(frame, top, state);
    draw_template_list(frame, list_area, state);
    draw_detail_pane(frame, detail_area, state);
}

fn draw_tabs(frame: &mut Frame, area: Rect, state: &ShellState) {
    let titles = vec!["Installed", "Registry"];
    let selected = match state.templates.tab {
        TemplateListTab::Installed => 0,
        TemplateListTab::Registry => 1,
    };
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" New project "),
        )
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn draw_template_list(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    let items: Vec<ListItem> = match state.templates.tab {
        TemplateListTab::Installed => state
            .templates
            .installed
            .iter()
            .map(|row| {
                let version = row
                    .version
                    .as_deref()
                    .map(|v| format!("@{v}"))
                    .unwrap_or_default();
                let yanked = if row.yanked { " [yanked]" } else { "" };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        row.short_name.as_str(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  {}{}", version, yanked)),
                ]))
            })
            .collect(),
        TemplateListTab::Registry => state
            .templates
            .registry
            .iter()
            .map(|row| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        row.package_id.as_str(),
                        Style::default().fg(Color::Cyan),
                    ),
                ]))
            })
            .collect(),
    };
    let title = format!(" {} ({}) ", tab_label(state.templates.tab), items.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_stateful_widget(list, area, &mut state.templates.list_state);
}

fn draw_detail_pane(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    let block = Block::default().borders(Borders::ALL).title(" Details ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    state.code_viewer.draw(frame, inner, None);
}

fn tab_label(tab: TemplateListTab) -> &'static str {
    match tab {
        TemplateListTab::Installed => "Installed",
        TemplateListTab::Registry => "Registry",
    }
}
