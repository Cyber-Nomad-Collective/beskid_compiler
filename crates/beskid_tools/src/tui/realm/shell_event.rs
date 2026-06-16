//! Tuirealm event mapping for the unified shell (no ratkit coordinator bridge).

use crossterm::event::{
    KeyCode, KeyEvent as CrosstermKeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    MouseButton, MouseEvent as CrosstermMouseEvent, MouseEventKind,
};
use tuirealm::event::{
    Event, Key, KeyEvent as RealmKeyEvent, KeyModifiers as RealmKeyModifiers, MediaKeyCode,
    MouseButton as RealmMouseButton, MouseEvent as RealmMouseEvent, MouseEventKind as RealmMouseKind,
    NoUserEvent,
};

use crate::tui::input::InputEvent;

/// Shell-level events delivered by the tuirealm listener.
#[derive(Debug, Clone)]
pub enum ShellRealmEvent {
    Input(InputEvent),
    Resize { width: u16, height: u16 },
    Tick,
}

/// Result of handling one shell event in the tuirealm component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOutcome {
    Continue,
    Redraw,
    Quit,
}

pub fn shell_event_from_realm(event: &Event<NoUserEvent>) -> Option<ShellRealmEvent> {
    match event {
        Event::Keyboard(key) => Some(ShellRealmEvent::Input(InputEvent::Key(
            realm_key_to_crossterm(key),
        ))),
        Event::Mouse(mouse) => Some(ShellRealmEvent::Input(InputEvent::Mouse(
            realm_mouse_to_crossterm(mouse),
        ))),
        Event::WindowResize(width, height) => Some(ShellRealmEvent::Resize {
            width: *width,
            height: *height,
        }),
        Event::Tick => Some(ShellRealmEvent::Tick),
        _ => None,
    }
}

pub fn mouse_is_click(mouse: &CrosstermMouseEvent) -> bool {
    matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Down(MouseButton::Right)
            | MouseEventKind::Down(MouseButton::Middle)
    )
}

pub fn mouse_is_move_or_drag(mouse: &CrosstermMouseEvent) -> bool {
    matches!(
        mouse.kind,
        MouseEventKind::Moved | MouseEventKind::Drag(_)
    )
}

pub fn mouse_is_inside(mouse: &CrosstermMouseEvent, rect: ratatui::layout::Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}

fn realm_key_to_crossterm(key: &RealmKeyEvent) -> CrosstermKeyEvent {
    CrosstermKeyEvent {
        code: realm_key_code(key.code),
        modifiers: realm_key_modifiers(key.modifiers),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn realm_key_modifiers(modifiers: RealmKeyModifiers) -> KeyModifiers {
    let mut out = KeyModifiers::empty();
    if modifiers.contains(RealmKeyModifiers::SHIFT) {
        out |= KeyModifiers::SHIFT;
    }
    if modifiers.contains(RealmKeyModifiers::CONTROL) {
        out |= KeyModifiers::CONTROL;
    }
    if modifiers.contains(RealmKeyModifiers::ALT) {
        out |= KeyModifiers::ALT;
    }
    out
}

fn realm_key_code(code: Key) -> KeyCode {
    match code {
        Key::Backspace => KeyCode::Backspace,
        Key::Enter => KeyCode::Enter,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::Tab => KeyCode::Tab,
        Key::BackTab => KeyCode::BackTab,
        Key::Delete => KeyCode::Delete,
        Key::Insert => KeyCode::Insert,
        Key::Function(n) => KeyCode::F(n.clamp(1, 12)),
        Key::Char(ch) => KeyCode::Char(ch),
        Key::Null => KeyCode::Null,
        Key::CapsLock => KeyCode::CapsLock,
        Key::ScrollLock => KeyCode::ScrollLock,
        Key::NumLock => KeyCode::NumLock,
        Key::PrintScreen => KeyCode::PrintScreen,
        Key::Pause => KeyCode::Pause,
        Key::Menu | Key::KeypadBegin => KeyCode::Null,
        Key::Media(media) => KeyCode::Media(realm_media_key(media)),
        Key::Esc => KeyCode::Esc,
        Key::ShiftLeft
        | Key::ShiftRight
        | Key::AltLeft
        | Key::AltRight
        | Key::CtrlLeft
        | Key::CtrlRight
        | Key::ShiftUp
        | Key::ShiftDown
        | Key::AltUp
        | Key::AltDown
        | Key::CtrlUp
        | Key::CtrlDown
        | Key::CtrlHome
        | Key::CtrlEnd => KeyCode::Null,
    }
}

fn realm_media_key(code: MediaKeyCode) -> crossterm::event::MediaKeyCode {
    use crossterm::event::MediaKeyCode as CrosstermMedia;
    match code {
        MediaKeyCode::Play => CrosstermMedia::Play,
        MediaKeyCode::Pause => CrosstermMedia::Pause,
        MediaKeyCode::PlayPause => CrosstermMedia::PlayPause,
        MediaKeyCode::Reverse => CrosstermMedia::Reverse,
        MediaKeyCode::Stop => CrosstermMedia::Stop,
        MediaKeyCode::FastForward => CrosstermMedia::FastForward,
        MediaKeyCode::Rewind => CrosstermMedia::Rewind,
        MediaKeyCode::TrackNext => CrosstermMedia::TrackNext,
        MediaKeyCode::TrackPrevious => CrosstermMedia::TrackPrevious,
        MediaKeyCode::Record => CrosstermMedia::Record,
        MediaKeyCode::LowerVolume => CrosstermMedia::LowerVolume,
        MediaKeyCode::RaiseVolume => CrosstermMedia::RaiseVolume,
        MediaKeyCode::MuteVolume => CrosstermMedia::MuteVolume,
    }
}

fn realm_mouse_to_crossterm(mouse: &RealmMouseEvent) -> CrosstermMouseEvent {
    CrosstermMouseEvent {
        kind: realm_mouse_kind(mouse.kind),
        column: mouse.column,
        row: mouse.row,
        modifiers: realm_key_modifiers(mouse.modifiers),
    }
}

fn realm_mouse_kind(kind: RealmMouseKind) -> MouseEventKind {
    match kind {
        RealmMouseKind::Down(button) => MouseEventKind::Down(realm_mouse_button(button)),
        RealmMouseKind::Up(button) => MouseEventKind::Up(realm_mouse_button(button)),
        RealmMouseKind::Drag(button) => MouseEventKind::Drag(realm_mouse_button(button)),
        RealmMouseKind::Moved => MouseEventKind::Moved,
        RealmMouseKind::ScrollDown => MouseEventKind::ScrollDown,
        RealmMouseKind::ScrollUp => MouseEventKind::ScrollUp,
        RealmMouseKind::ScrollLeft => MouseEventKind::ScrollLeft,
        RealmMouseKind::ScrollRight => MouseEventKind::ScrollRight,
    }
}

fn realm_mouse_button(button: RealmMouseButton) -> MouseButton {
    match button {
        RealmMouseButton::Left => MouseButton::Left,
        RealmMouseButton::Right => MouseButton::Right,
        RealmMouseButton::Middle => MouseButton::Middle,
    }
}
