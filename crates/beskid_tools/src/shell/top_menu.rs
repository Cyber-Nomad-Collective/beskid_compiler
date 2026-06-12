//! Pinned top menu bar — nav registry roots with dropdown for groups.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

use super::layout::pages::PagesDoc;
use super::nav::{NavAction, NavItemDescriptor, NavRegistry};
use super::panel_style::popover_block;
use super::key_bindings::ShortcutBindings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopMenuAction {
    None,
    Redraw,
    SwitchPage(String),
    OpenOverlay(String),
    RunCli(Vec<String>),
}

#[derive(Debug, Clone)]
struct MenuEntry {
    label: String,
    action: NavAction,
    children: Vec<MenuEntry>,
}

#[derive(Debug, Clone, Copy)]
struct ItemHit {
    index: usize,
    rect: Rect,
}

pub struct ShellTopMenu {
    entries: Vec<MenuEntry>,
    selected: usize,
    dropdown_open: bool,
    dropdown_selected: usize,
    menu_focused: bool,
    bar_rect: Rect,
    item_hits: Vec<ItemHit>,
    dropdown_rect: Option<Rect>,
}

impl ShellTopMenu {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            dropdown_open: false,
            dropdown_selected: 0,
            menu_focused: false,
            bar_rect: Rect::default(),
            item_hits: Vec::new(),
            dropdown_rect: None,
        }
    }

    pub fn rebuild(&mut self, registry: &NavRegistry, pages: &PagesDoc) {
        self.entries = build_menu_entries(registry, pages);
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.dropdown_open = false;
        self.dropdown_selected = 0;
    }

    pub fn is_active(&self) -> bool {
        self.menu_focused || self.dropdown_open
    }

    pub fn dropdown_open(&self) -> bool {
        self.dropdown_open
    }

    pub fn dropdown_rect(&self) -> Option<Rect> {
        self.dropdown_rect
    }

    pub fn close(&mut self) {
        self.menu_focused = false;
        self.dropdown_open = false;
    }

    fn toggle_menu_focus(&mut self) -> TopMenuAction {
        self.menu_focused = !self.menu_focused;
        if !self.menu_focused {
            self.dropdown_open = false;
        }
        TopMenuAction::Redraw
    }

    pub fn handle_key(&mut self, key: KeyEvent, bindings: &ShortcutBindings) -> TopMenuAction {
        if key.kind != KeyEventKind::Press {
            return TopMenuAction::None;
        }
        if bindings.toggles_menu(&key) {
            return self.toggle_menu_focus();
        }
        if !self.is_active() {
            return TopMenuAction::None;
        }
        if self.dropdown_open {
            return self.handle_dropdown_key(key);
        }
        match key.code {
            KeyCode::Esc => {
                self.close();
                TopMenuAction::Redraw
            }
            KeyCode::Left => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                TopMenuAction::Redraw
            }
            KeyCode::Right => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
                TopMenuAction::Redraw
            }
            KeyCode::Down | KeyCode::Enter => self.open_or_activate_selected(),
            _ => TopMenuAction::Redraw,
        }
    }

    fn handle_dropdown_key(&mut self, key: KeyEvent) -> TopMenuAction {
        let child_count = self
            .entries
            .get(self.selected)
            .map(|e| e.children.len())
            .unwrap_or(0);
        match key.code {
            KeyCode::Esc => {
                self.dropdown_open = false;
                TopMenuAction::Redraw
            }
            KeyCode::Up => {
                if self.dropdown_selected > 0 {
                    self.dropdown_selected -= 1;
                }
                TopMenuAction::Redraw
            }
            KeyCode::Down => {
                if self.dropdown_selected + 1 < child_count {
                    self.dropdown_selected += 1;
                }
                TopMenuAction::Redraw
            }
            KeyCode::Enter => self.activate_dropdown_selected(),
            _ => TopMenuAction::Redraw,
        }
    }

    pub fn handle_mouse(&mut self, column: u16, row: u16) -> TopMenuAction {
        if let Some(rect) = self.dropdown_rect {
            if point_in_rect(column, row, rect) {
                if let Some(index) = dropdown_row_index(rect, column, row) {
                    let child_count = self
                        .entries
                        .get(self.selected)
                        .map(|e| e.children.len())
                        .unwrap_or(0);
                    if index < child_count {
                        self.dropdown_selected = index;
                        return self.activate_dropdown_selected();
                    }
                }
                return TopMenuAction::Redraw;
            }
            if self.dropdown_open {
                self.dropdown_open = false;
                return TopMenuAction::Redraw;
            }
        }

        for hit in &self.item_hits {
            if point_in_rect(column, row, hit.rect) {
                self.menu_focused = true;
                if self.selected != hit.index {
                    self.selected = hit.index;
                    self.dropdown_open = false;
                    return TopMenuAction::Redraw;
                }
                return self.open_or_activate_selected();
            }
        }

        if point_in_rect(column, row, self.bar_rect) {
            return TopMenuAction::Redraw;
        }

        TopMenuAction::None
    }

    fn open_or_activate_selected(&mut self) -> TopMenuAction {
        let has_children = self
            .entries
            .get(self.selected)
            .is_some_and(|e| !e.children.is_empty());
        if has_children {
            self.dropdown_open = true;
            self.dropdown_selected = 0;
            return TopMenuAction::Redraw;
        }
        let action = self
            .entries
            .get(self.selected)
            .map(|e| e.action.clone())
            .unwrap_or(NavAction::Group);
        self.close();
        self.dispatch_action(action)
    }

    fn activate_dropdown_selected(&mut self) -> TopMenuAction {
        let action = self
            .entries
            .get(self.selected)
            .and_then(|e| e.children.get(self.dropdown_selected))
            .map(|c| c.action.clone());
        let Some(action) = action else {
            return TopMenuAction::Redraw;
        };
        self.close();
        self.dispatch_action(action)
    }

    fn dispatch_action(&mut self, action: NavAction) -> TopMenuAction {
        match action {
            NavAction::Page(id) => TopMenuAction::SwitchPage(id),
            NavAction::Overlay(id) | NavAction::Widget(id) => TopMenuAction::OpenOverlay(id),
            NavAction::Cli(argv) => TopMenuAction::RunCli(argv),
            NavAction::Group => TopMenuAction::Redraw,
        }
    }

    pub fn render_menu_row(&mut self, area: Rect, frame: &mut Frame) {
        self.bar_rect = area;
        self.item_hits.clear();
        if area.width < 4 || area.height == 0 {
            self.dropdown_rect = None;
            return;
        }
        let mut spans = Vec::new();
        let mut x = area.x + 1;
        for (index, entry) in self.entries.iter().enumerate() {
            let has_children = !entry.children.is_empty();
            let label = if has_children {
                format!("{} ▾", entry.label)
            } else {
                entry.label.clone()
            };
            let width = label.chars().count() as u16 + 2;
            let style = if index == self.selected && self.menu_focused {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if index == self.selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            if x + width <= area.x + area.width {
                self.item_hits.push(ItemHit {
                    index,
                    rect: Rect::new(x, area.y, width, area.height),
                });
            }
            spans.push(Span::styled(label, style));
            spans.push(Span::raw("  "));
            x = x.saturating_add(width + 2);
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// Render dropdown above all other content (call last in draw pass).
    pub fn render_dropdown(&mut self, frame_area: Rect, frame: &mut Frame) {
        self.dropdown_rect = None;
        if !self.dropdown_open {
            return;
        }
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        if entry.children.is_empty() {
            return;
        }
        let width = entry
            .children
            .iter()
            .map(|c| c.label.chars().count())
            .max()
            .unwrap_or(12)
            .max(entry.label.chars().count()) as u16
            + 4;
        let height = entry.children.len() as u16 + 2;
        let drop_x = self
            .item_hits
            .iter()
            .find(|h| h.index == self.selected)
            .map(|h| h.rect.x)
            .unwrap_or(self.bar_rect.x + 1);
        let drop_y = self.bar_rect.y.saturating_add(self.bar_rect.height);
        let drop = Rect::new(
            drop_x,
            drop_y,
            width.min(frame_area.width.saturating_sub(drop_x.saturating_sub(frame_area.x))),
            height.min(frame_area.height.saturating_sub(drop_y.saturating_sub(frame_area.y))),
        );
        self.dropdown_rect = Some(drop);
        frame.render_widget(Clear, drop);
        let items: Vec<ListItem> = entry
            .children
            .iter()
            .enumerate()
            .map(|(idx, child)| {
                let style = if idx == self.dropdown_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(child.label.as_str()).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(popover_block(&entry.label)),
            drop,
        );
    }
}

impl Default for ShellTopMenu {
    fn default() -> Self {
        Self::new()
    }
}

fn point_in_rect(column: u16, row: u16, rect: Rect) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn dropdown_row_index(rect: Rect, _column: u16, row: u16) -> Option<usize> {
    if rect.height < 3 {
        return None;
    }
    let inner_y = row.saturating_sub(rect.y + 1);
    if inner_y == 0 {
        return None;
    }
    let index = inner_y.saturating_sub(1) as usize;
    Some(index)
}

fn build_menu_entries(registry: &NavRegistry, pages: &PagesDoc) -> Vec<MenuEntry> {
    let mut merged = NavRegistry::new();
    merged.merge_pages(pages);
    for item in registry.items() {
        merged.register(item.clone());
    }
    let mut roots: Vec<_> = merged.roots().into_iter().cloned().collect();
    roots.sort_by_key(|item| item.order);

    let mut entries = Vec::new();
    for root in roots {
        if root.id == "beskid" && matches!(root.action, NavAction::Group) {
            let mut children: Vec<_> = merged.children_of(&root.id).into_iter().cloned().collect();
            children.sort_by_key(|c| c.order);
            for child in children {
                entries.push(menu_entry_from(&merged, &child));
            }
        } else {
            entries.push(menu_entry_from(&merged, &root));
        }
    }
    entries
}

fn menu_entry_from(registry: &NavRegistry, item: &NavItemDescriptor) -> MenuEntry {
    let mut children: Vec<_> = registry.children_of(&item.id).into_iter().cloned().collect();
    children.sort_by_key(|c| c.order);
    MenuEntry {
        label: item.label.clone(),
        action: item.action.clone(),
        children: children
            .into_iter()
            .filter(|c| !matches!(c.action, NavAction::Group))
            .map(|c| MenuEntry {
                label: c.label,
                action: c.action,
                children: Vec::new(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::layout::pages::parse_pages;
    use crate::shell::layout::pages::EMBEDDED_HI_PAGES;
    use crate::shell::nav::NavRegistry;

    #[test]
    fn menu_includes_compiler_and_boards() {
        let registry = NavRegistry::new();
        let pages = parse_pages(EMBEDDED_HI_PAGES).expect("pages");
        let menu = build_menu_entries(&registry, &pages);
        assert!(menu.iter().any(|e| e.label == "Compiler"));
        assert!(menu.iter().any(|e| e.label == "Boards"));
    }

    #[test]
    fn compiler_opens_dropdown_with_graphs() {
        let registry = NavRegistry::new();
        let pages = parse_pages(EMBEDDED_HI_PAGES).expect("pages");
        let entries = build_menu_entries(&registry, &pages);
        let compiler = entries.iter().find(|e| e.label == "Compiler").expect("compiler");
        assert!(compiler.children.iter().any(|c| c.label == "Graphs"));
    }
}
