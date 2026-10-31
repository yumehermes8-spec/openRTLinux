//! Synthetic input through `/dev/uinput` - the Linux stand-in for `SendInput`.
//!
//! One difference matters for the whole port: Windows can type a Unicode
//! character directly (`KEYEVENTF_UNICODE`), so RightType never cares which
//! keyboard is selected while it writes. uinput has no such thing - it sends
//! *key codes*, and the desktop turns them into characters with whatever layout
//! is active. So the sequence here is always
//! **switch the layout first, then type the keys that name the text in it**
//! (the character -> key direction is [`crate::layout::Layout::key_of`]).

use std::io;
use std::thread::sleep;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

use crate::linuxos::keytable;
use crate::layout::Layout;

pub struct Injector {
    dev: VirtualDevice,
}

/// One character -> (evdev code, shift). Whitespace has no entry in a layout
/// table: the core's tables only map the keys a layout remaps.
fn resolve(ch: char, table: &dyn Layout) -> Result<(u16, bool), (char, String)> {
    let key = match table.key_of(ch) {
        Some(key) => key,
        None if ch == ' ' || ch == '\n' || ch == '\t' => ch,
        None => return Err((ch, "this layout cannot type that character".into())),
    };
    match key {
        ' ' => Ok((KeyCode::KEY_SPACE.0, false)),
        '\n' => Ok((KeyCode::KEY_ENTER.0, false)),
        '\t' => Ok((KeyCode::KEY_TAB.0, false)),
        other => {
            keytable::key_for(other).ok_or_else(|| (ch, format!("no key code for {other:?}")))
        }
    }
}

impl Injector {
    pub fn new() -> io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in keytable::codes() {
            keys.insert(KeyCode::new(code));
        }
        for extra in [
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_BACKSPACE,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_ENTER,
            KeyCode::KEY_TAB,
            KeyCode::KEY_GRAVE,
            KeyCode::KEY_CAPSLOCK,
            KeyCode::KEY_LEFTCTRL,
            KeyCode::KEY_LEFTALT,
            KeyCode::KEY_LEFTMETA,
        ] {
            keys.insert(extra);
        }
        let dev = VirtualDevice::builder()?
            .name("openRTLinux virtual keyboard")
            .with_keys(&keys)?
            .build()?;
        Ok(Injector { dev })
    }

    fn key(&mut self, code: u16, down: bool) -> io::Result<()> {
        self.dev.emit(&[InputEvent::new(
            EventType::KEY.0,
            code,
            if down { 1 } else { 0 },
        )])
    }

    /// One key press *and* release in a single `emit` (one syscall, one SYN):
    /// the application sees an ordinary tap, we save the round trip.
    fn tap(&mut self, code: u16, shift: bool) -> io::Result<()> {
        let shift_code = KeyCode::KEY_LEFTSHIFT.0;
        let mut events = Vec::with_capacity(4);
        let mut push = |code: u16, value: i32| {
            events.push(InputEvent::new(EventType::KEY.0, code, value));
        };
        if shift {
            push(shift_code, 1);
        }
        push(code, 1);
        push(code, 0);
        if shift {
            push(shift_code, 0);
        }
        self.dev.emit(&events)
    }

    /// Hold every code but the last, tap the last, release the rest - needed for
    /// layout switch chords such as Super+Space.
    pub fn chord(&mut self, codes: &[u16]) -> io::Result<()> {
        let (mods, key) = match codes.split_last() {
            Some((key, mods)) => (mods, *key),
            None => return Ok(()),
        };
        for code in mods {
            self.key(*code, true)?;
        }
        self.key(key, true)?;
        self.key(key, false)?;
        for code in mods.iter().rev() {
            self.key(*code, false)?;
        }
        Ok(())
    }

    pub fn backspaces(&mut self, count: usize) -> io::Result<()> {
        // 1 ms apart: fast enough to feel instant, slow enough that an
        // application's own key-repeat/debounce logic still sees every press.
        for _ in 0..count {
            self.tap(KeyCode::KEY_BACKSPACE.0, false)?;
            sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    /// Can `text` be typed at all with `table` selected?
    ///
    /// Asked before anything is deleted: a plan that deletes the user's word and
    /// then cannot type the replacement has destroyed their text.
    pub fn can_type(text: &str, table: &dyn Layout) -> Result<(), (char, String)> {
        for ch in text.chars() {
            resolve(ch, table)?;
        }
        Ok(())
    }

    /// Type `text` with `table` selected: each character is turned into the
    /// physical key that produces it in that layout.
    pub fn type_in_layout(
        &mut self,
        text: &str,
        table: &dyn Layout,
    ) -> Result<(), (char, String)> {
        Self::can_type(text, table)?;
        for ch in text.chars() {
            let (code, shift) = resolve(ch, table)?;
            if let Err(e) = self.tap(code, shift) {
                return Err((ch, e.to_string()));
            }
            sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::thai_variant;

    /// What the focused application would receive for `text` typed in `table`
    /// while `held` modifiers are physically down.
    ///
    /// This is the app's side of the injection: every tap becomes a key code, and
    /// the desktop then applies *the modifiers that are really down* - which is
    /// why typing while Shift is held does not produce what we asked for.
    fn app_receives(text: &str, table: &dyn Layout, held_shift: bool) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            let (code, shift) = resolve(ch, table).expect("typeable");
            let us = keytable::us_char(code, shift || held_shift).expect("a US char");
            let produced = table.char_of(us).unwrap_or(us);
            out.push(produced);
        }
        out
    }

    const FLOWER: &str = "\u{e14}\u{e2d}\u{e01}\u{e44}\u{e21}\u{e49}";

    #[test]
    fn typing_thai_with_shift_held_arrives_as_the_upper_row_glyphs() {
        // the bug the user hit: the hotkey is Shift+Backspace, so Shift is still
        // down when we type, and every Thai letter arrives shifted
        let scrambled = app_receives(FLOWER, crate::layout::thai_table(thai_variant()), true);
        assert_ne!(scrambled, FLOWER, "typing with Shift held must change the text");
        assert_eq!(scrambled.chars().count(), FLOWER.chars().count());
    }

    #[test]
    fn typing_after_the_modifier_is_released_arrives_exactly() {
        let exact = app_receives(FLOWER, crate::layout::thai_table(thai_variant()), false);
        assert_eq!(exact, FLOWER);
    }

    #[test]
    fn latin_with_shift_held_arrives_in_capitals() {
        let shouted = app_receives("bask", crate::layout::us_table(), true);
        assert_eq!(shouted, "BASK");
    }

    #[test]
    fn a_word_is_typeable_with_its_own_layout_and_a_space() {
        // the common case: one script plus the boundary character
        let thai = crate::layout::thai_table(thai_variant());
        let us = crate::layout::us_table();
        assert!(Injector::can_type(&format!("{FLOWER} "), thai).is_ok());
        assert!(Injector::can_type("bask ", us).is_ok());
    }

    #[test]
    fn a_mixed_script_string_needs_more_than_one_layout() {
        // Kedmanee gives every key a Thai letter, so Latin is *not* typeable with
        // it - and one plan can only ever select one layout. This is why the plan
        // runner validates the whole replacement before deleting anything: a
        // string like this is refused instead of half-typed. (Typing across
        // scripts would need a switch in the middle of a plan - not implemented.)
        let thai = crate::layout::thai_table(thai_variant());
        let us = crate::layout::us_table();
        let mixed = format!("{FLOWER} x");
        assert!(Injector::can_type(&mixed, thai).is_err());
        assert!(Injector::can_type(&mixed, us).is_err());
    }

    #[test]
    fn can_type_rejects_what_the_layout_cannot_reach() {
        // a Thai-only glyph asked of the US table is not typeable: the plan must
        // never delete the user's word to then fail
        let err = Injector::can_type("\u{e01}", crate::layout::us_table());
        assert!(err.is_err(), "the US table cannot type a Thai letter");
    }
}
