//! pckg registry browser: package list + detail/readme pane.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::input;
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
        let mut effects = Vec::new();
        match msg {
            ShellMessage::SetOverlayVisible {
                kind: OverlayKind::Pckg,
                visible: true,
            } => {
                state.set_overlay_visible(OverlayKind::Pckg, true);
                state.focus_overlay(OverlayKind::Pckg);
                if !state.pckg.catalog_loaded && !state.pckg.loading {
                    effects.push(ShellEffect::FetchPckgCatalog);
                }
            }
            ShellMessage::PckgCatalogLoaded(packages) => {
                state.pckg.loading = false;
                state.pckg.catalog_loaded = true;
                state.pckg.error = None;
                state.pckg.packages.clone_from(packages);
                if !state.pckg.packages.is_empty() && state.pckg.list_state.selected().is_none() {
                    state.pckg.list_state.select(Some(0));
                }
                state.sync_pckg_detail_viewer();
                if let Some(id) = state.pckg.selected_package_id().map(str::to_owned) {
                    effects.push(ShellEffect::FetchPckgDetails { package_id: id });
                }
            }
            ShellMessage::PckgCatalogFailed(error) => {
                state.pckg.loading = false;
                state.pckg.error = Some(error.clone());
            }
            ShellMessage::PckgDetailsLoaded(details) => {
                state.pckg.detail_loading = false;
                state.pckg.detail = Some(details.as_ref().clone());
                state.sync_pckg_detail_viewer();
            }
            ShellMessage::PckgDetailsFailed(error) => {
                state.pckg.detail_loading = false;
                state.pckg.status = Some(error.clone());
            }
            ShellMessage::EnterProjectWizard => {}
            _ => {}
        }
    effects
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_pckg_overlay_input(event, state)
}

pub fn render(area: Rect, frame: &mut Frame, state: &mut ShellState) {
        let [list_area, detail_area] = Layout::horizontal([
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ])
        .areas(area);

        draw_package_list(frame, list_area, state);
        draw_detail_pane(frame, detail_area, state);
}

fn draw_package_list(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    let title = if state.pckg.search_query.is_empty() {
        format!(" Packages ({}) ", state.pckg.packages.len())
    } else {
        format!(" Search: {} ", state.pckg.search_query)
    };
    let items: Vec<ListItem> = state
        .pckg
        .packages
        .iter()
        .map(|pkg| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    pkg.name.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    pkg.category.as_str(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_stateful_widget(list, area, &mut state.pckg.list_state);
}

fn draw_detail_pane(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    let Some(detail) = state.pckg.detail.as_ref() else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Package details ");
        let text = if state.pckg.detail_loading {
            "Loading details…"
        } else {
            "Select a package to view details and readme."
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
            block.inner(area),
        );
        frame.render_widget(block, area);
        return;
    };

    let [meta, readme] = Layout::vertical([Constraint::Length(6), Constraint::Min(4)]).areas(area);
    let meta_block = Block::default().borders(Borders::ALL).title(format!(
        " {} ",
        detail.package.name
    ));
    let version_count = detail.versions.len();
    let meta_lines = vec![
        Line::from(detail.package.description.as_str()),
        Line::from(""),
        Line::from(vec![
            Span::styled("category ", Style::default().fg(Color::DarkGray)),
            Span::raw(detail.package.category.as_str()),
            Span::raw("  "),
            Span::styled("versions ", Style::default().fg(Color::DarkGray)),
            Span::raw(version_count.to_string()),
            Span::raw("  "),
            Span::styled("downloads ", Style::default().fg(Color::DarkGray)),
            Span::raw(detail.package.total_downloads.to_string()),
        ]),
        Line::from(vec![
            Span::styled("tags ", Style::default().fg(Color::DarkGray)),
            Span::raw(detail.package.tags.join(", ")),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(meta_lines).block(meta_block),
        meta,
    );
    state.code_viewer.draw(frame, readme, Some("readme"));
}
