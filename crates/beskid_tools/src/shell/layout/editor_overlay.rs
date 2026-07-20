//! Layout editor drawer overlay (templates, widgets, saved layouts, structure).

use std::fs;
use std::path::PathBuf;

use super::editor::{LayoutEditorState, LayoutOverlayTab};
use super::model::{BoardNode, BoardV2Doc, NodeKind};
use super::templates::LAYOUT_TEMPLATES;
use crate::shell::descriptor::WidgetDescriptor;
use crate::shell::scope::{ShellScope, user_data_dir};
use crate::tui::overlay_chrome::{draw_backdrop, hotkey, render_overlay_panel};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs};

#[derive(Default)]
pub struct LayoutEditorOverlay {
    pub template_selected: usize,
    pub widget_selected: usize,
    pub layout_selected: usize,
    pub structure_selected: usize,
    saved_boards: Vec<PathBuf>,
}

impl LayoutEditorOverlay {
    pub fn refresh_saved_boards(&mut self, scope: &ShellScope) {
        self.saved_boards = list_saved_boards(scope);
        if self.layout_selected >= self.saved_boards.len() {
            self.layout_selected = self.saved_boards.len().saturating_sub(1);
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        editor: &mut LayoutEditorState,
        doc: &BoardV2Doc,
    ) -> LayoutOverlayAction {
        if key.kind != KeyEventKind::Press {
            return LayoutOverlayAction::None;
        }
        let structure_len = flatten_structure(doc).len();
        match key.code {
            KeyCode::Tab | KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                editor.overlay_tab = editor.overlay_tab.next();
                LayoutOverlayAction::Redraw
            }
            KeyCode::BackTab | KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                editor.overlay_tab = editor.overlay_tab.prev();
                LayoutOverlayAction::Redraw
            }
            KeyCode::Char('1') => {
                editor.overlay_tab = LayoutOverlayTab::Templates;
                LayoutOverlayAction::Redraw
            }
            KeyCode::Char('2') => {
                editor.overlay_tab = LayoutOverlayTab::Widgets;
                LayoutOverlayAction::Redraw
            }
            KeyCode::Char('3') => {
                editor.overlay_tab = LayoutOverlayTab::Layouts;
                LayoutOverlayAction::Redraw
            }
            KeyCode::Char('4') => {
                editor.overlay_tab = LayoutOverlayTab::Structure;
                LayoutOverlayAction::Redraw
            }
            KeyCode::Down => {
                self.bump_selected(editor.overlay_tab, 1, doc, structure_len);
                LayoutOverlayAction::Redraw
            }
            KeyCode::Up => {
                self.bump_selected(editor.overlay_tab, usize::MAX, doc, structure_len);
                LayoutOverlayAction::Redraw
            }
            KeyCode::Enter => self.activate(editor.overlay_tab, doc),
            KeyCode::Char('a') if editor.overlay_tab == LayoutOverlayTab::Widgets => {
                widget_descriptor(self.widget_selected)
                    .map(|d| LayoutOverlayAction::AddWidget(d.id))
                    .unwrap_or(LayoutOverlayAction::Redraw)
            }
            _ => LayoutOverlayAction::None,
        }
    }

    fn bump_selected(
        &mut self,
        tab: LayoutOverlayTab,
        delta: usize,
        _doc: &BoardV2Doc,
        structure_len: usize,
    ) {
        match tab {
            LayoutOverlayTab::Templates => {
                let n = LAYOUT_TEMPLATES.len();
                if n == 0 {
                    return;
                }
                if delta == 1 {
                    self.template_selected = (self.template_selected + 1).min(n - 1);
                } else {
                    self.template_selected = self.template_selected.saturating_sub(1);
                }
            }
            LayoutOverlayTab::Widgets => {
                let n = widget_count();
                if n == 0 {
                    return;
                }
                if delta == 1 {
                    self.widget_selected = (self.widget_selected + 1).min(n - 1);
                } else {
                    self.widget_selected = self.widget_selected.saturating_sub(1);
                }
            }
            LayoutOverlayTab::Layouts => {
                let n = self.saved_boards.len();
                if n == 0 {
                    return;
                }
                if delta == 1 {
                    self.layout_selected = (self.layout_selected + 1).min(n - 1);
                } else {
                    self.layout_selected = self.layout_selected.saturating_sub(1);
                }
            }
            LayoutOverlayTab::Structure => {
                if structure_len == 0 {
                    return;
                }
                if delta == 1 {
                    self.structure_selected = (self.structure_selected + 1).min(structure_len - 1);
                } else {
                    self.structure_selected = self.structure_selected.saturating_sub(1);
                }
            }
        }
    }

    fn activate(&self, tab: LayoutOverlayTab, doc: &BoardV2Doc) -> LayoutOverlayAction {
        match tab {
            LayoutOverlayTab::Templates => LAYOUT_TEMPLATES
                .get(self.template_selected)
                .map(|t| LayoutOverlayAction::ApplyTemplate(t.id))
                .unwrap_or(LayoutOverlayAction::Redraw),
            LayoutOverlayTab::Widgets => widget_descriptor(self.widget_selected)
                .map(|d| LayoutOverlayAction::SetWidget(d.id))
                .unwrap_or(LayoutOverlayAction::Redraw),
            LayoutOverlayTab::Layouts => self
                .saved_boards
                .get(self.layout_selected)
                .cloned()
                .map(LayoutOverlayAction::LoadBoard)
                .unwrap_or(LayoutOverlayAction::Redraw),
            LayoutOverlayTab::Structure => {
                let flat = flatten_structure(doc);
                if let Some((_, id, _)) = flat.get(self.structure_selected) {
                    LayoutOverlayAction::FocusNode(id.clone())
                } else {
                    LayoutOverlayAction::Redraw
                }
            }
        }
    }

    pub fn render(
        &self,
        area: Rect,
        frame: &mut Frame,
        editor: &LayoutEditorState,
        doc: &BoardV2Doc,
        descriptors: &[WidgetDescriptor],
    ) {
        if !editor.drawer_visible {
            return;
        }
        draw_backdrop(frame, area);
        let drawer = right_drawer_rect(40, area);
        frame.render_widget(Clear, drawer);

        let tab_titles: Vec<Line> = LayoutOverlayTab::ALL
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let num = format!("{}:", i + 1);
                let style = if *tab == editor.overlay_tab {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(vec![
                    Span::styled(num, style),
                    Span::styled(tab.label(), style),
                ])
            })
            .collect();
        let tabs = Tabs::new(tab_titles)
            .select(
                LayoutOverlayTab::ALL
                    .iter()
                    .position(|t| *t == editor.overlay_tab)
                    .unwrap_or(0),
            )
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );

        let hotkeys = &[
            hotkey("w", "hide drawer"),
            hotkey("Ctrl+Tab", "next tab"),
            hotkey("↑/↓", "select"),
            hotkey("Enter", "apply"),
            hotkey("a", "add panel"),
            hotkey("Esc", "exit edit"),
        ];
        render_overlay_panel(frame, drawer, " Layout editor ", hotkeys, |body, f| {
            let [tab_area, list_area, detail_area] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(6),
                Constraint::Length(4),
            ])
            .areas(body);
            f.render_widget(
                tabs.block(Block::default().borders(Borders::BOTTOM)),
                tab_area,
            );
            match editor.overlay_tab {
                LayoutOverlayTab::Templates => self.render_templates(list_area, detail_area, f),
                LayoutOverlayTab::Widgets => {
                    self.render_widgets(list_area, detail_area, f, descriptors)
                }
                LayoutOverlayTab::Layouts => self.render_layouts(list_area, detail_area, f),
                LayoutOverlayTab::Structure => {
                    self.render_structure(list_area, detail_area, f, doc)
                }
            }
        });
    }

    fn render_templates(&self, list: Rect, detail: Rect, frame: &mut Frame) {
        let items: Vec<ListItem> = LAYOUT_TEMPLATES
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let style = if i == self.template_selected {
                    Style::default().bg(Color::DarkGray).fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} — {}", t.title, t.id)).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Templates ")),
            list,
        );
        if let Some(t) = LAYOUT_TEMPLATES.get(self.template_selected) {
            frame.render_widget(
                Paragraph::new(t.description).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Description "),
                ),
                detail,
            );
        }
    }

    fn render_widgets(
        &self,
        list: Rect,
        detail: Rect,
        frame: &mut Frame,
        descriptors: &[WidgetDescriptor],
    ) {
        let items: Vec<ListItem> = descriptors
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let style = if i == self.widget_selected {
                    Style::default().bg(Color::DarkGray).fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} {} — {}", d.icon, d.title, d.id)).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Widgets ")),
            list,
        );
        if let Some(d) = descriptors.get(self.widget_selected) {
            frame.render_widget(
                Paragraph::new(d.description).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {} ", d.id)),
                ),
                detail,
            );
        }
    }

    fn render_layouts(&self, list: Rect, detail: Rect, frame: &mut Frame) {
        let items: Vec<ListItem> = if self.saved_boards.is_empty() {
            vec![ListItem::new("(no saved board files)")]
        } else {
            self.saved_boards
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let style = if i == self.layout_selected {
                        Style::default().bg(Color::DarkGray).fg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    let label = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("board.bsol");
                    ListItem::new(format!("{label}  {}", path.display())).style(style)
                })
                .collect()
        };
        frame.render_widget(
            List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Saved layouts "),
            ),
            list,
        );
        let hint = if let Some(path) = self.saved_boards.get(self.layout_selected) {
            format!("Load {} into current scope", path.display())
        } else {
            "Save a layout to ~/.beskid/data/boards/ or scope .beskid/board.bsol".into()
        };
        frame.render_widget(
            Paragraph::new(hint).block(Block::default().borders(Borders::ALL).title(" Hint ")),
            detail,
        );
    }

    fn render_structure(&self, list: Rect, detail: Rect, frame: &mut Frame, doc: &BoardV2Doc) {
        let flat = flatten_structure(doc);
        let items: Vec<ListItem> = flat
            .iter()
            .enumerate()
            .map(|(i, (depth, id, label))| {
                let indent = "  ".repeat(*depth);
                let style = if i == self.structure_selected {
                    Style::default().bg(Color::DarkGray).fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{indent}{label} ({id})")).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Structure ")),
            list,
        );
        let detail_text = flat
            .get(self.structure_selected)
            .and_then(|(_, id, _)| doc.node(id))
            .map(node_detail)
            .unwrap_or_else(|| "Select a node".into());
        frame.render_widget(
            Paragraph::new(detail_text)
                .block(Block::default().borders(Borders::ALL).title(" Node ")),
            detail,
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutOverlayAction {
    None,
    Redraw,
    ApplyTemplate(&'static str),
    SetWidget(&'static str),
    AddWidget(&'static str),
    LoadBoard(PathBuf),
    FocusNode(String),
}

fn right_drawer_rect(percent_x: u16, area: Rect) -> Rect {
    let chunks = Layout::horizontal([
        Constraint::Percentage(100 - percent_x),
        Constraint::Percentage(percent_x),
    ])
    .split(area);
    chunks[1]
}

fn list_saved_boards(scope: &ShellScope) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let scope_path = scope.board_config_path();
    if scope_path.is_file() {
        paths.push(scope_path);
    }
    let boards_dir = user_data_dir().join("boards");
    if boards_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&boards_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "bsol")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn flatten_structure(doc: &BoardV2Doc) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    walk_node(doc, &doc.root, 0, &mut out);
    out
}

fn walk_node(doc: &BoardV2Doc, id: &str, depth: usize, out: &mut Vec<(usize, String, String)>) {
    let Some(node) = doc.node(id) else {
        return;
    };
    let label = structure_label(node);
    out.push((depth, id.to_string(), label));
    for child in &node.children {
        walk_node(doc, child, depth + 1, out);
    }
}

fn structure_label(node: &BoardNode) -> String {
    match node.kind {
        NodeKind::Panel => node.widget.as_deref().unwrap_or("panel").to_string(),
        other => other.as_str().to_string(),
    }
}

fn node_detail(node: &BoardNode) -> String {
    let mut lines = vec![format!("kind: {}", node.kind.as_str())];
    if let Some(widget) = &node.widget {
        lines.push(format!("widget: {widget}"));
    }
    if let Some(grow) = node.grow {
        lines.push(format!("grow: {grow}"));
    }
    if !node.children.is_empty() {
        lines.push(format!("children: {}", node.children.join(", ")));
    }
    lines.join("\n")
}

fn widget_count() -> usize {
    crate::shell::descriptor::BUILTIN_DESCRIPTORS.len()
}

fn widget_descriptor(index: usize) -> Option<&'static WidgetDescriptor> {
    crate::shell::descriptor::BUILTIN_DESCRIPTORS.get(index)
}
