//! Mouse hit regions for shortcut hints (footer, chrome, panels).

use ratatui::layout::Rect;

use super::primitives::HotkeyItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutClickAction {
    OpenPalette,
    ToggleHelp,
    Quit,
    RebindShortcut(usize),
}

#[derive(Debug, Default)]
pub struct ShortcutClickTargets {
    regions: Vec<(Rect, ShortcutClickAction)>,
}

impl ShortcutClickTargets {
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn add_rect(&mut self, rect: Rect, action: ShortcutClickAction) {
        if rect.width > 0 && rect.height > 0 {
            self.regions.push((rect, action));
        }
    }

    pub fn add_row(&mut self, area: Rect, row_index: u16, action: ShortcutClickAction) {
        let y = area.y.saturating_add(row_index);
        if y >= area.y.saturating_add(area.height) {
            return;
        }
        self.add_rect(
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            action,
        );
    }

    pub fn hit(&self, column: u16, row: u16) -> Option<ShortcutClickAction> {
        self.regions.iter().rev().find_map(|(rect, action)| {
            if point_in_rect(column, row, *rect) {
                Some(*action)
            } else {
                None
            }
        })
    }
}

pub fn action_for_hotkey_description(description: &str) -> Option<ShortcutClickAction> {
    match description {
        "Command palette" => Some(ShortcutClickAction::OpenPalette),
        "Shortcut help" => Some(ShortcutClickAction::ToggleHelp),
        "Quit" => Some(ShortcutClickAction::Quit),
        _ => None,
    }
}

pub fn register_footer_clicks(
    targets: &mut ShortcutClickTargets,
    area: Rect,
    items: &[HotkeyItem],
) {
    if area.height == 0 {
        return;
    }
    let mut x = area.x.saturating_add(1);
    let y = area.y;
    for item in items {
        let key_w = item.key.chars().count() as u16;
        let desc_w = item.description.chars().count() as u16;
        let segment_w = key_w.saturating_add(1).saturating_add(desc_w).saturating_add(2);
        if let Some(action) = action_for_hotkey_description(&item.description) {
            let width = segment_w.min(area.width.saturating_sub(x.saturating_sub(area.x)));
            targets.add_rect(Rect { x, y, width, height: 1 }, action);
        }
        x = x.saturating_add(segment_w);
    }
}

pub fn register_help_overlay_clicks(
    targets: &mut ShortcutClickTargets,
    area: Rect,
    items: &[HotkeyItem],
) {
    if area.height < 3 {
        return;
    }
    let body_y = area.y.saturating_add(1);
    let body_height = area.height.saturating_sub(2);
    for (idx, item) in items.iter().enumerate() {
        let row = body_y.saturating_add(idx as u16);
        if row >= body_y.saturating_add(body_height) {
            break;
        }
        if let Some(action) = action_for_hotkey_description(&item.description) {
            targets.add_rect(
                Rect {
                    x: area.x.saturating_add(1),
                    y: row,
                    width: area.width.saturating_sub(2),
                    height: 1,
                },
                action,
            );
        }
    }
}

fn point_in_rect(column: u16, row: u16, rect: Rect) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_prefers_topmost_region() {
        let mut targets = ShortcutClickTargets::default();
        targets.add_rect(Rect::new(0, 0, 10, 1), ShortcutClickAction::Quit);
        targets.add_rect(Rect::new(0, 0, 10, 1), ShortcutClickAction::OpenPalette);
        assert_eq!(
            targets.hit(5, 0),
            Some(ShortcutClickAction::OpenPalette)
        );
    }
}
