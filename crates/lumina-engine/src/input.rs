//! Translation from window-system input to the engine's input events.

use std::str::FromStr;

use servo::{
    Code, CompositionEvent, CompositionState, Cursor, DevicePoint, ImeEvent, InputEvent, Key,
    KeyState, KeyboardEvent, Location, Modifiers, MouseButton, MouseButtonAction, MouseButtonEvent,
    MouseLeftViewportEvent, MouseMoveEvent, NamedKey, WheelDelta, WheelEvent, WheelMode,
};
use winit::event::{ElementState, Ime, KeyEvent, MouseScrollDelta};
use winit::keyboard::{
    Key as WinitKey, KeyLocation, ModifiersState, NamedKey as WinitNamedKey, PhysicalKey,
};
use winit::window::CursorIcon;

/// How many pixels one notch of a line-based scroll wheel moves the page.
const WHEEL_LINE_PIXELS: f32 = 76.0;

pub(crate) fn pointer_moved(point: DevicePoint) -> InputEvent {
    InputEvent::MouseMove(MouseMoveEvent::new(point.into()))
}

pub(crate) fn pointer_left() -> InputEvent {
    InputEvent::MouseLeftViewport(MouseLeftViewportEvent::default())
}

pub(crate) fn mouse_button(
    state: ElementState,
    button: winit::event::MouseButton,
    point: DevicePoint,
) -> InputEvent {
    let action = match state {
        ElementState::Pressed => MouseButtonAction::Down,
        ElementState::Released => MouseButtonAction::Up,
    };
    let button = match button {
        winit::event::MouseButton::Left => MouseButton::Primary,
        winit::event::MouseButton::Right => MouseButton::Secondary,
        winit::event::MouseButton::Middle => MouseButton::Auxiliary,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(other) => MouseButton::Other(other),
    };
    InputEvent::MouseButton(MouseButtonEvent::new(action, button, point.into()))
}

pub(crate) fn wheel(delta: MouseScrollDelta, point: DevicePoint) -> InputEvent {
    let (x, y, mode) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (
            f64::from(x * WHEEL_LINE_PIXELS),
            f64::from(y * WHEEL_LINE_PIXELS),
            WheelMode::DeltaLine,
        ),
        MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y, WheelMode::DeltaPixel),
    };
    InputEvent::Wheel(WheelEvent::new(
        WheelDelta { x, y, z: 0.0, mode },
        point.into(),
    ))
}

pub(crate) fn modifiers(state: ModifiersState) -> Modifiers {
    let mut modifiers = Modifiers::empty();
    modifiers.set(Modifiers::SHIFT, state.shift_key());
    modifiers.set(Modifiers::CONTROL, state.control_key());
    modifiers.set(Modifiers::ALT, state.alt_key());
    modifiers.set(Modifiers::META, state.super_key());
    modifiers
}

pub(crate) fn keyboard(event: &KeyEvent, modifiers: Modifiers, composing: bool) -> InputEvent {
    InputEvent::Keyboard(KeyboardEvent::new_without_event(
        match event.state {
            ElementState::Pressed => KeyState::Down,
            ElementState::Released => KeyState::Up,
        },
        logical_key(&event.logical_key),
        physical_code(event.physical_key),
        match event.location {
            KeyLocation::Standard => Location::Standard,
            KeyLocation::Left => Location::Left,
            KeyLocation::Right => Location::Right,
            KeyLocation::Numpad => Location::Numpad,
        },
        modifiers,
        event.repeat,
        composing,
    ))
}

/// Translates one step of the system input method into the composition
/// events the page expects, given whether a composition is already open.
/// Returns the events to deliver and whether a composition is open afterwards.
///
/// Pages expect a strict `start`, `update`…, `end` sequence, while the window
/// system reports only the text being composed and the text finally committed,
/// so the opening event is supplied here.
pub(crate) fn composition(composing: bool, ime: &Ime) -> (Vec<InputEvent>, bool) {
    let event = |state, data: &str| {
        InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state,
            data: data.to_owned(),
        }))
    };

    match ime {
        Ime::Enabled => (Vec::new(), composing),
        // An empty pre-edit with nothing open is the window system clearing
        // its state before a commit; it carries no composition.
        Ime::Preedit(text, _) if text.is_empty() && !composing => (Vec::new(), false),
        Ime::Preedit(text, _) => {
            let mut events = Vec::new();
            if !composing {
                events.push(event(CompositionState::Start, ""));
            }
            events.push(event(CompositionState::Update, text));
            (events, true)
        }
        Ime::Commit(text) => {
            let mut events = Vec::new();
            if !composing {
                events.push(event(CompositionState::Start, ""));
            }
            events.push(event(CompositionState::End, text));
            (events, false)
        }
        Ime::Disabled if composing => (vec![InputEvent::Ime(ImeEvent::Dismissed)], false),
        Ime::Disabled => (Vec::new(), false),
    }
}

/// The key's meaning under the current layout.
///
/// winit and the engine both name keys after the W3C UI Events specification,
/// so apart from the few names listed here the variant name carries over
/// unchanged. Anything the engine does not know becomes `Unidentified`.
fn logical_key(key: &WinitKey) -> Key {
    match key {
        WinitKey::Character(text) => Key::Character(text.to_string()),
        WinitKey::Named(WinitNamedKey::Space) => Key::Character(" ".to_owned()),
        WinitKey::Named(WinitNamedKey::Super) => Key::Named(NamedKey::Meta),
        WinitKey::Named(named) => {
            Key::Named(NamedKey::from_str(&format!("{named:?}")).unwrap_or(NamedKey::Unidentified))
        }
        WinitKey::Dead(_) => Key::Named(NamedKey::Dead),
        WinitKey::Unidentified(_) => Key::Named(NamedKey::Unidentified),
    }
}

/// The key's position on the keyboard, independent of layout.
fn physical_code(key: PhysicalKey) -> Code {
    match key {
        PhysicalKey::Code(winit::keyboard::KeyCode::SuperLeft) => Code::MetaLeft,
        PhysicalKey::Code(winit::keyboard::KeyCode::SuperRight) => Code::MetaRight,
        PhysicalKey::Code(code) => {
            Code::from_str(&format!("{code:?}")).unwrap_or(Code::Unidentified)
        }
        PhysicalKey::Unidentified(_) => Code::Unidentified,
    }
}

pub(crate) fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        // winit has no "hidden" icon; hiding is a separate window call that
        // the embedder does not make yet, so fall back to the arrow.
        Cursor::None | Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        Cursor::Help => CursorIcon::Help,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Move => CursorIcon::Move,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EResize => CursorIcon::EResize,
        Cursor::NResize => CursorIcon::NResize,
        Cursor::NeResize => CursorIcon::NeResize,
        Cursor::NwResize => CursorIcon::NwResize,
        Cursor::SResize => CursorIcon::SResize,
        Cursor::SeResize => CursorIcon::SeResize,
        Cursor::SwResize => CursorIcon::SwResize,
        Cursor::WResize => CursorIcon::WResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
    }
}

#[cfg(test)]
mod tests {
    use winit::keyboard::{KeyCode, SmolStr};

    use super::*;

    #[test]
    fn letters_digits_and_editing_keys_keep_their_codes() {
        let cases = [
            (KeyCode::KeyA, Code::KeyA),
            (KeyCode::Digit7, Code::Digit7),
            (KeyCode::Enter, Code::Enter),
            (KeyCode::Backspace, Code::Backspace),
            (KeyCode::Tab, Code::Tab),
            (KeyCode::Escape, Code::Escape),
            (KeyCode::ArrowLeft, Code::ArrowLeft),
            (KeyCode::F5, Code::F5),
            (KeyCode::ShiftLeft, Code::ShiftLeft),
            (KeyCode::NumpadEnter, Code::NumpadEnter),
            (KeyCode::SuperLeft, Code::MetaLeft),
        ];
        for (winit_code, expected) in cases {
            assert_eq!(physical_code(PhysicalKey::Code(winit_code)), expected);
        }
    }

    #[test]
    fn named_keys_keep_their_meaning() {
        let cases = [
            (WinitNamedKey::Enter, NamedKey::Enter),
            (WinitNamedKey::Backspace, NamedKey::Backspace),
            (WinitNamedKey::Tab, NamedKey::Tab),
            (WinitNamedKey::Escape, NamedKey::Escape),
            (WinitNamedKey::ArrowDown, NamedKey::ArrowDown),
            (WinitNamedKey::PageUp, NamedKey::PageUp),
            (WinitNamedKey::Home, NamedKey::Home),
            (WinitNamedKey::Delete, NamedKey::Delete),
            (WinitNamedKey::Shift, NamedKey::Shift),
            (WinitNamedKey::Control, NamedKey::Control),
            (WinitNamedKey::F11, NamedKey::F11),
            (WinitNamedKey::Super, NamedKey::Meta),
        ];
        for (winit_key, expected) in cases {
            assert_eq!(
                logical_key(&WinitKey::Named(winit_key)),
                Key::Named(expected)
            );
        }
    }

    #[test]
    fn typed_text_including_turkish_letters_passes_through() {
        for text in ["a", "ğ", "İ", "ş", " "] {
            let key = if text == " " {
                WinitKey::Named(WinitNamedKey::Space)
            } else {
                WinitKey::Character(SmolStr::new(text))
            };
            assert_eq!(logical_key(&key), Key::Character(text.to_owned()));
        }
    }

    /// Reduces composition events to `(state, text)` pairs for comparison.
    fn steps(events: &[InputEvent]) -> Vec<(Option<CompositionState>, String)> {
        events
            .iter()
            .map(|event| match event {
                InputEvent::Ime(ImeEvent::Composition(composition)) => {
                    (Some(composition.state), composition.data.clone())
                }
                InputEvent::Ime(ImeEvent::Dismissed) => (None, String::new()),
                other => panic!("not an input method event: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn composing_then_committing_forms_one_complete_sequence() {
        let (first, composing) = composition(false, &Ime::Preedit("に".to_owned(), None));
        assert!(composing);
        assert_eq!(
            steps(&first),
            [
                (Some(CompositionState::Start), String::new()),
                (Some(CompositionState::Update), "に".to_owned()),
            ]
        );

        let (second, composing) = composition(composing, &Ime::Preedit("にほ".to_owned(), None));
        assert!(composing);
        assert_eq!(
            steps(&second),
            [(Some(CompositionState::Update), "にほ".to_owned())]
        );

        let (cleared, composing) = composition(composing, &Ime::Preedit(String::new(), None));
        assert!(composing);
        assert_eq!(
            steps(&cleared),
            [(Some(CompositionState::Update), String::new())]
        );

        let (last, composing) = composition(composing, &Ime::Commit("日本".to_owned()));
        assert!(!composing);
        assert_eq!(
            steps(&last),
            [(Some(CompositionState::End), "日本".to_owned())]
        );
    }

    #[test]
    fn a_commit_without_a_preedit_still_opens_and_closes() {
        let (cleared, composing) = composition(false, &Ime::Preedit(String::new(), None));
        assert!(!composing);
        assert!(cleared.is_empty());

        let (events, composing) = composition(composing, &Ime::Commit("ş".to_owned()));
        assert!(!composing);
        assert_eq!(
            steps(&events),
            [
                (Some(CompositionState::Start), String::new()),
                (Some(CompositionState::End), "ş".to_owned()),
            ]
        );
    }

    #[test]
    fn disabling_mid_composition_dismisses_it() {
        let (events, composing) = composition(true, &Ime::Disabled);
        assert!(!composing);
        assert_eq!(steps(&events), [(None, String::new())]);

        let (events, composing) = composition(false, &Ime::Disabled);
        assert!(!composing);
        assert!(events.is_empty());
    }

    #[test]
    fn modifier_state_is_carried_over() {
        let state = ModifiersState::SHIFT | ModifiersState::CONTROL;
        let converted = modifiers(state);
        assert!(converted.contains(Modifiers::SHIFT));
        assert!(converted.contains(Modifiers::CONTROL));
        assert!(!converted.contains(Modifiers::ALT));
        assert!(!converted.contains(Modifiers::META));
    }
}
