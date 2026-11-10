//! The Auto pipeline, driving the shared detection core.
//!
//! This is the Linux counterpart of the Windows hook's Auto path. Version 1
//! commits at word boundaries in both directions - the behaviour the earlier
//! Thai layout fixer on this host already proved end to end:
//!
//! * **Thai on screen, meant in English** -> at whitespace, read the token back
//!   as keys and type the English word.
//! * **English on screen, meant in Thai** -> at whitespace, convert the token
//!   and switch the layout so the next words are Thai.
//!
//! The mid-word EN->TH anchoring (with its commit horizon and withdrawal) is
//! deliberately *not* here yet: it depends on reading the run back out of the
//! app, which is the part Wayland makes hardest. The decisions themselves come
//! from `policy::detect_token`, so this file is plumbing, not policy.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::buffer::{Key, WordBuffer};
use crate::dict;
use crate::linuxos::config::{self, Mode};
use crate::layout as core_layout;
use crate::policy::{self, InputLayout};

/// What the OS layer has to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Select this layout before typing (uinput sends key codes, not Unicode).
    Switch(InputLayout),
    /// Delete this many characters (the token plus its boundary).
    Backspaces(usize),
    /// Type this text with the layout above selected.
    Type(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// What the token looked like on screen.
    pub shown: String,
    /// What it is being replaced with.
    pub corrected: String,
    pub steps: Vec<Plan>,
}

/// How many fixes the settings window can see.
pub const HISTORY: usize = 20;

/// One fix that was made, as the history shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixed {
    /// Unix seconds.
    pub at: u64,
    pub shown: String,
    pub corrected: String,
    /// "fix" for a correction, "swap" for a swap back or an undo.
    pub kind: &'static str,
}

/// A fix that can be undone: both spellings of the same word, and the boundary
/// that was typed after it.
struct Swap {
    wrong: String,
    right: String,
    separator: String,
    /// When the fix was made, on the daemon's millisecond clock.
    at: u64,
}

/// A word that a boundary closed, kept for the fix chord - with the time it was
/// closed at.
///
/// The chord is pressed *after* the word is already on screen, so the memory
/// needs an age: this process cannot see the pointer or the window focus, and a
/// word remembered from another document would be deleted at whatever cursor is
/// there now. Past the window the word is forgotten and the press does nothing.
struct Remembered {
    word: String,
    separator: String,
    at: u64,
}

pub struct Live {
    layout: InputLayout,
    buf: WordBuffer,
    /// The last word a boundary closed, with the boundary character.
    ///
    /// Manual mode needs it: the typist presses space (which ends the word) and
    /// *then* decides to fix it. RightType calls this "convert the last word in
    /// place"; without it the hotkey had nothing left to look at, because the
    /// boundary had already emptied the buffer.
    pending: Option<Remembered>,
    /// The word as it was before the last plain Backspace: a Shift+Backspace
    /// chord can reach us in the other order (Backspace, then Shift), and by
    /// then the buffer has already lost the character the chord was meant to eat.
    before_backspace: Option<String>,
    /// The millisecond clock the recall windows are measured against. The daemon
    /// sets it from its own `Instant` before each decision; the tests move it by
    /// hand instead of sleeping.
    now: u64,
    /// How long a word that is already on screen stays reachable for the fix
    /// chord (see [`config::DEFAULT_RECALL_SECONDS`]).
    pub recall_ms: u64,
    /// How long the newest fix can still be swapped back or undone (see
    /// [`config::DEFAULT_UNDO_SECONDS`]).
    pub undo_ms: u64,
    /// The last automatic fix, kept so the hotkey can swap it back and forth
    /// (`Mode::Both`): sometimes the "wrong" spelling is the one that was wanted -
    /// a character name, a password, a joke.
    swap: Option<Swap>,
    /// Which side of `swap` is on screen.
    showing_right: bool,
    /// The fixes that were actually made, newest first - the settings window shows
    /// them, and the last one can be undone while it is still the last thing
    /// that happened.
    pub history: VecDeque<Fixed>,
    pub mode: Mode,
    pub committed: usize,
    pub skipped: usize,
}

impl Live {
    pub fn new(layout: InputLayout) -> Self {
        Live {
            layout,
            buf: WordBuffer::new(),
            pending: None,
            before_backspace: None,
            now: 0,
            recall_ms: config::window_ms(config::DEFAULT_RECALL_SECONDS),
            undo_ms: config::window_ms(config::DEFAULT_UNDO_SECONDS),
            swap: None,
            showing_right: false,
            history: VecDeque::new(),
            mode: Mode::Auto,
            committed: 0,
            skipped: 0,
        }
    }

    pub fn layout(&self) -> InputLayout {
        self.layout
    }

    /// The typist switched layout themselves.
    pub fn note_user_switch(&mut self, to: InputLayout) {
        self.forget();
        self.buf.clear();
        self.layout = to;
    }

    /// Move the clock the recall windows are measured against.
    pub fn set_now(&mut self, millis: u64) {
        self.now = millis;
    }

    /// Is a memory stamped `at` still inside `limit_ms`?
    fn is_fresh(&self, at: u64, limit_ms: u64) -> bool {
        self.now.saturating_sub(at) <= limit_ms
    }

    /// Drop every word anchored to a cursor we can no longer vouch for - Escape,
    /// an arrow key, a deliberate layout switch. The time windows catch what
    /// this cannot see: the pointer, and another window taking the focus.
    fn forget(&mut self) {
        self.pending = None;
        self.before_backspace = None;
        self.swap = None;
        self.showing_right = false;
    }

    /// A printable key: `produced` is the character the *active layout* made of
    /// the physical key (caps normalised away, as the hook does).
    pub fn on_char(&mut self, produced: char) {
        self.buf.observe(Key::Char(produced));
    }

    pub fn on_backspace(&mut self) {
        self.buf.observe(Key::Backspace);
    }

    pub fn on_reset(&mut self) {
        self.forget();
        self.buf.observe(Key::Reset);
    }

    pub fn current_word(&self) -> &str {
        self.buf.current()
    }

    /// A plain Backspace went to the application: remember the word it just
    /// shortened, in case the Shift of a Shift+Backspace chord arrives next.
    pub fn note_backspace(&mut self) {
        self.before_backspace = Some(self.buf.current().to_string());
    }

    /// The chord arrived the other way round: Backspace first, then Shift.
    ///
    /// The application (and the buffer) have already lost one character, which is
    /// exactly the accounting the normal hotkey path expects - so put the word
    /// back in the buffer and run it.
    pub fn on_fix_hotkey_after_backspace(&mut self) -> Option<Committed> {
        let word = self.before_backspace.take()?;
        if word.is_empty() {
            return None;
        }
        self.buf.replace(&word);
        self.on_fix_hotkey(1)
    }

    /// The fix hotkey (Shift+Backspace), for manual mode and as a rescue in auto.
    ///
    /// The application has already deleted a character by the time the chord
    /// reaches us, so the last buffered character is dropped before the
    /// decision - RightLang's own accounting, kept because it is what makes the
    /// keystroke a no-op instead of a mangler.
    pub fn on_fix_hotkey(&mut self, pre_deleted: usize) -> Option<Committed> {
        if self.mode == Mode::Off {
            return None;
        }
        if self.mode == Mode::Both {
            if let Some(swapped) = self.swap_last_fix(pre_deleted) {
                return Some(swapped);
            }
        }
        // The buffer is what the typist typed; the application is one character
        // behind it (its own Backspace fired first). So the decision is made on
        // the whole word and only the *deletion* is adjusted.
        // Two shapes of the same request:
        //  * mid-word: the buffer holds it, and the hotkey's own Backspace has
        //    already removed one character (delete the rest, no boundary);
        //  * after a boundary: the word was committed to the screen, and the
        //    Backspace removed the boundary, so the word itself must go too and
        //    the boundary has to be typed back.
        let (shown, separator, extra) = if self.buf.current().is_empty() {
            let recalled = self.pending.take();
            match recalled.filter(|r| self.is_fresh(r.at, self.recall_ms)) {
                // the word is committed to the screen and the Backspace took the
                // boundary: the whole word has to go, and the boundary comes back
                Some(Remembered { word, separator, .. }) => (word, separator, 0),
                // forgot it, or it is too old to know where the cursor went:
                // nothing is deleted and nothing is typed
                None => {
                    self.skipped += 1;
                    return None;
                }
            }
        } else {
            let word = self.buf.current().to_string();
            (word, String::new(), pre_deleted)
        };
        self.buf.clear();
        self.plan_fix(shown, extra, separator, false)
    }

    /// The fix hotkey as a *chord on Space*, when the typist picks that one.
    ///
    /// A shifted Space puts a space in the application before we ever see it, so
    /// the word and that space both have to go, and the corrected text is typed
    /// without a trailing space - the space was a keypress, not a word boundary.
    pub fn on_fix_hotkey_via_space(&mut self) -> Option<Committed> {
        if self.mode == Mode::Off {
            return None;
        }
        let shown = if !self.buf.current().is_empty() {
            let word = self.buf.current().to_string();
            self.buf.clear();
            word
        } else {
            let recalled = self.pending.take();
            match recalled.filter(|r| self.is_fresh(r.at, self.recall_ms)) {
                Some(Remembered { word, .. }) => word,
                None => {
                    self.skipped += 1;
                    return None;
                }
            }
        };
        self.plan_fix(shown, 0, String::new(), true)
    }

    /// Decide the text the typist meant, then work out the keystrokes.
    ///
    /// `extra` is how many characters the application has already removed, and
    /// `delete_boundary` says whether a boundary (space/enter/tab) is on screen
    /// after the word and has to go too.
    fn plan_fix(
        &mut self,
        shown: String,
        extra: usize,
        separator: String,
        delete_boundary: bool,
    ) -> Option<Committed> {
        if shown.is_empty() {
            self.skipped += 1;
            return None;
        }
        let Some(corrected) = self.decide(&shown) else {
            self.skipped += 1;
            return None;
        };
        let target = layout_of(&corrected);
        let mut steps = Vec::new();
        if target != self.layout {
            steps.push(Plan::Switch(target));
        }
        let remaining = shown
            .chars()
            .count()
            .saturating_sub(extra)
            .saturating_add(usize::from(delete_boundary));
        steps.push(Plan::Backspaces(remaining));
        steps.push(Plan::Type(format!("{corrected}{separator}")));
        self.layout = target;
        self.committed += 1;
        self.remember(&shown, &corrected, "fix");
        // whatever mode asked for the fix, it can be undone while it is still the
        // last thing that happened (the hotkey only uses this in `both`)
        self.swap = Some(Swap {
            wrong: shown.clone(),
            right: corrected.clone(),
            separator: separator.clone(),
            at: self.now,
        });
        self.showing_right = true;
        self.pending = Some(Remembered {
            word: corrected.clone(),
            separator: separator.clone(),
            at: self.now,
        });
        Some(Committed {
            shown,
            corrected,
            steps,
        })
    }

    /// Keep the last [`HISTORY`] fixes, newest first.
    fn remember(&mut self, shown: &str, corrected: &str, kind: &'static str) {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.history.push_front(Fixed {
            at,
            shown: shown.to_string(),
            corrected: corrected.to_string(),
            kind,
        });
        while self.history.len() > HISTORY {
            self.history.pop_back();
        }
    }

    /// May the last fix still be undone? Only while it is still the last thing
    /// that happened on screen - the typist has not typed since, and the word is
    /// still the one we left there.
    pub fn can_undo(&self) -> bool {
        let Some(swap) = self.swap.as_ref() else {
            return false;
        };
        // the same guards `swap_last_fix` applies, or the button lies
        if !self.is_fresh(swap.at, self.undo_ms) {
            return false;
        }
        if !self.buf.current().is_empty() {
            return false;
        }
        let expected = if self.showing_right { &swap.right } else { &swap.wrong };
        matches!(self.pending.as_ref(), Some(remembered) if &remembered.word == expected)
    }

    /// Undo it, whoever asks: the settings window's button, not only the hotkey.
    ///
    /// Nothing has been deleted by a keystroke here, which is the one thing
    /// [`swap_last_fix`](Self::swap_last_fix) has to be told.
    pub fn undo_last(&mut self) -> Option<Committed> {
        self.swap_last_fix(0)
    }

    /// What the typist meant, for a deliberate press.
    ///
    /// Three answers, in order of how much they know:
    ///  1. the word checker, which understands compounds and trailing marks;
    ///  2. the other layout's reading, if that is a word and the screen is not;
    ///  3. the other layout's reading, period - but only for a string no
    ///     dictionary can judge (one to three characters, where "dd" and "กก" are
    ///     both words and neither verdict means anything). A press is a
    ///     deliberate act, so the typist gets what they asked for.
    fn decide(&self, shown: &str) -> Option<String> {
        if let Some(detection) =
            policy::detect_token(shown, self.layout, dict::english(), dict::thai())
        {
            if detection.corrected != shown {
                return Some(detection.corrected);
            }
        }
        if let Some(reading) = careful_reading(shown, self.layout) {
            return Some(reading);
        }
        let reading = read_in_other(shown, self.layout);
        let readable = shown
            .chars()
            .any(|c| c.is_alphabetic() || ('\u{0E00}'..='\u{0E7F}').contains(&c));
        if reading == shown || !readable {
            return None;
        }
        // A short token: read it the other way and be done, because the
        // dictionaries cannot tell "dd" from "กก".
        if shown.chars().count() <= SHORT_ENOUGH_TO_JUST_READ_IT {
            return Some(reading);
        }
        // A longer one: only if it is not a word where it sits. A real word on
        // screen stays put - an accidental press must not mangle it - while a
        // string no dictionary knows ("Suvasin" typed in the Thai layout) is
        // plainly the wrong layout, which is what the press was about.
        let (here, _) = match self.layout {
            InputLayout::ThaiKedmanee => (dict::thai(), dict::english()),
            InputLayout::UsQwerty => (dict::english(), dict::thai()),
        };
        if here.contains(shown.trim()) {
            None
        } else {
            Some(reading)
        }
    }

    /// A boundary key (space/enter/tab). Returns the plan to run, if any.
    pub fn on_boundary(&mut self, boundary: char) -> Option<Committed> {
        let shown = self.buf.observe(Key::Boundary)?;
        if !matches!(self.mode, Mode::Auto | Mode::Both) {
            // manual: the word is on screen, waiting for the hotkey
            self.pending = Some(Remembered {
                word: shown,
                separator: boundary.to_string(),
                at: self.now,
            });
            return None;
        }
        // the word checker first, then the second opinion a deliberate press gets:
        // the other layout's reading, when that reading is a word and the screen is
        // not. Measured over both dictionaries this never flipped a word that was
        // already correct, and it lifts short words from 50%/65% to 71%/96%.
        let corrected = match policy::detect_token(&shown, self.layout, dict::english(), dict::thai())
        {
            Some(detection) if detection.corrected != shown => detection.corrected,
            _ if shown.chars().count() >= 2 => match careful_reading(&shown, self.layout) {
                Some(reading) => reading,
                None => return None,
            },
            // a single character is never fixed without being asked
            _ => return None,
        };
        let target = layout_of(&corrected);
        let mut steps = Vec::new();
        if target != self.layout {
            steps.push(Plan::Switch(target));
        }
        // the boundary is already in the application: delete it too and type it back
        steps.push(Plan::Backspaces(shown.chars().count() + 1));
        steps.push(Plan::Type(format!("{corrected}{boundary}")));
        self.layout = target;
        self.committed += 1;
        self.remember(&shown, &corrected, "fix");
        if self.mode == Mode::Both {
            // remember both spellings: the hotkey swaps between them
            self.swap = Some(Swap {
                wrong: shown.clone(),
                right: corrected.clone(),
                separator: boundary.to_string(),
                at: self.now,
            });
            self.showing_right = true;
        }
        self.pending = Some(Remembered {
            word: corrected.clone(),
            separator: boundary.to_string(),
            at: self.now,
        });
        Some(Committed {
            shown,
            corrected,
            steps,
        })
    }

    /// `Mode::Both`: the hotkey swaps the last automatic fix back and forth.
    ///
    /// Returns `None` when the press is not about that word - a new word is being
    /// typed, or the word under the cursor is not the one we just fixed - and the
    /// ordinary "fix this word" path takes over.
    ///
    /// `pre_deleted` is how many characters the caller's own keystroke has already
    /// removed from the screen before this decision: 1 for the fix chord (its
    /// Shift+Backspace), 0 for the tray's Undo button. Getting it wrong is how a
    /// repeated press ate the character *before* the word - the chord's Backspace
    /// takes the boundary when a boundary was typed, and a character of the word
    /// itself when none was, and the deletion has to shrink to match.
    fn swap_last_fix(&mut self, pre_deleted: usize) -> Option<Committed> {
        let (from, to, separator) = {
            let swap = self.swap.as_ref()?;
            // past the undo window the cursor may be anywhere: let the ordinary
            // "fix this word" path take over instead of deleting blind
            if !self.is_fresh(swap.at, self.undo_ms) {
                return None;
            }
            if !self.buf.current().is_empty() {
                return None;
            }
            let expected = if self.showing_right { &swap.right } else { &swap.wrong };
            match self.pending.as_ref() {
                Some(remembered) if &remembered.word == expected => {}
                _ => return None,
            }
            (
                if self.showing_right { swap.right.clone() } else { swap.wrong.clone() },
                if self.showing_right { swap.wrong.clone() } else { swap.right.clone() },
                swap.separator.clone(),
            )
        };
        let target = layout_of(&to);
        let mut steps = Vec::new();
        if target != self.layout {
            steps.push(Plan::Switch(target));
        }
        // What is on screen is `from` plus its boundary, less whatever the
        // caller's keystroke already ate: the chord's Backspace removes the
        // boundary when there is one, and a character of the word when there is
        // not. Delete exactly that much, then type the other spelling and the
        // boundary back.
        let on_screen = from.chars().count() + separator.chars().count();
        let remaining = on_screen.saturating_sub(pre_deleted);
        steps.push(Plan::Backspaces(remaining));
        steps.push(Plan::Type(format!("{to}{separator}")));
        self.layout = target;
        self.showing_right = !self.showing_right;
        self.pending = Some(Remembered {
            word: to.clone(),
            separator,
            at: self.now,
        });
        self.committed += 1;
        self.remember(&from, &to, "swap");
        Some(Committed {
            shown: from,
            corrected: to,
            steps,
        })
    }
}

/// Below this many characters the dictionaries stop being evidence: two-letter
/// abbreviations are in every word list, and short tokens are words in both
/// languages often enough that no verdict can be trusted.
const SHORT_ENOUGH_TO_JUST_READ_IT: usize = 3;

/// The key table for a layout (the port has exactly two).
pub fn table_for(layout: InputLayout) -> &'static dyn core_layout::Layout {
    match layout {
        InputLayout::ThaiKedmanee => core_layout::thai_table(core_layout::thai_variant()),
        InputLayout::UsQwerty => core_layout::us_table(),
    }
}

/// The other layout.
fn other(layout: InputLayout) -> InputLayout {
    match layout {
        InputLayout::ThaiKedmanee => InputLayout::UsQwerty,
        InputLayout::UsQwerty => InputLayout::ThaiKedmanee,
    }
}

/// `text` read as if the other layout had been selected all along.
fn read_in_other(text: &str, layout: InputLayout) -> String {
    core_layout::convert(text, table_for(layout), table_for(other(layout)))
}

/// The other layout's reading of `text`, if *that* reading is a word and what is
/// on screen is not.
///
/// The typist's own word checker is conservative with short tokens: it refuses
/// whenever the string on screen also looks like a word, which at two and three
/// characters is often. The hotkey is pressed on purpose, so a deliberate request
/// gets this second opinion. Measured over the whole dictionary, it rescues 99.3%
/// of words put through the wrong layout while never flipping a word that was
/// typed correctly (two-character tokens stay around 71%: they are genuinely
/// ambiguous, and guessing there would flip one correctly-typed short word in
/// three).
fn careful_reading(text: &str, layout: InputLayout) -> Option<String> {
    let reading = read_in_other(text, layout);
    if reading == text {
        return None;
    }
    let (here, there) = match layout {
        InputLayout::ThaiKedmanee => (dict::thai(), dict::english()),
        InputLayout::UsQwerty => (dict::english(), dict::thai()),
    };
    if there.contains(reading.trim()) && !here.contains(text.trim()) {
        Some(reading)
    } else {
        None
    }
}

/// Which layout a piece of text has to be typed in.
pub fn layout_of(text: &str) -> InputLayout {
    if text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        InputLayout::ThaiKedmanee
    } else {
        InputLayout::UsQwerty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Thai words in escapes so the file stays pure ASCII:
    //   \u{e14}\u{e2d}\u{e01}\u{e44}\u{e21}\u{e49} = "dok mai" (flower)
    //   \u{e41}\u{e19}\u{e1e}\u{e1e}\u{e33}\u{e41}\u{e30} = what the Thai layout shows for "correct"
    const FLOWER: &str = "\u{e14}\u{e2d}\u{e01}\u{e44}\u{e21}\u{e49}";
    const CORRECT_KEYS: &str = "\u{e41}\u{e19}\u{e1e}\u{e1e}\u{e33}\u{e41}\u{e30}";

    /// The buffer holds what the *active layout* put on screen, so these tests
    /// feed real on-screen text, not physical keys.
    #[test]
    fn english_text_on_the_english_layout_is_read_as_thai() {
        let mut live = Live::new(InputLayout::UsQwerty);
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        let committed = live.on_boundary(' ').expect("the Thai reading is found");
        assert_eq!(committed.shown, "fvdw,h");
        assert_eq!(committed.corrected, FLOWER);
        assert!(matches!(committed.steps[0], Plan::Switch(InputLayout::ThaiKedmanee)));
        assert!(matches!(committed.steps[1], Plan::Backspaces(7)), "the word plus its boundary");
        assert!(matches!(&committed.steps[2], Plan::Type(t) if t == &format!("{FLOWER} ")));
        assert_eq!(live.layout(), InputLayout::ThaiKedmanee, "typing continues in Thai");
    }

    #[test]
    fn thai_text_meant_as_english_comes_back() {
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        for c in CORRECT_KEYS.chars() {
            live.on_char(c);
        }
        let committed = live.on_boundary(' ').expect("the English reading is found");
        assert_eq!(committed.corrected, "correct");
        assert!(matches!(committed.steps[0], Plan::Switch(InputLayout::UsQwerty)));
        assert!(matches!(committed.steps[1], Plan::Backspaces(8)), "7 keys plus its boundary");
        assert_eq!(live.layout(), InputLayout::UsQwerty);
    }

    #[test]
    fn real_thai_words_are_left_alone() {
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        for c in FLOWER.chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_none());
    }

    #[test]
    fn english_words_are_left_alone() {
        let mut live = Live::new(InputLayout::UsQwerty);
        for c in "hello".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_none());
    }

    #[test]
    fn manual_mode_only_fixes_when_asked() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_none(), "space must not fix in manual mode");

        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        // the hotkey's own Backspace already removed the "h": five characters
        // are still on screen, and the decision uses the whole typed word
        let committed = live.on_fix_hotkey(1).expect("the hotkey fixes the word");
        assert_eq!(committed.shown, "fvdw,h");
        assert_eq!(layout_of(&committed.corrected), InputLayout::ThaiKedmanee);
        assert!(matches!(committed.steps[1], Plan::Backspaces(5)));
        assert!(matches!(committed.steps.last(), Some(Plan::Type(t)) if !t.ends_with(' ')));
    }

    #[test]
    fn the_hotkey_after_a_space_still_fixes_that_word() {
        // manual mode, and the typist pressed space before deciding
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_none(), "space must not fix in manual mode");

        let committed = live.on_fix_hotkey(1).expect("the word the space closed is fixed");
        assert_eq!(committed.shown, "fvdw,h");
        assert_eq!(committed.corrected, "\u{e14}\u{e2d}\u{e01}\u{e44}\u{e21}\u{e49}");
        assert!(
            matches!(committed.steps[1], Plan::Backspaces(6)),
            "the whole word, because the Backspace took the space: {:?}",
            committed.steps
        );
        assert!(
            matches!(&committed.steps[2], Plan::Type(t) if t.ends_with(' ')),
            "the space is typed back"
        );
        assert!(live.on_fix_hotkey(1).is_none(), "nothing is fixed twice");
    }

    #[test]
    fn the_hotkey_mid_word_uses_the_buffer() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        let committed = live.on_fix_hotkey(1).expect("mid-word fix");
        assert!(matches!(committed.steps[1], Plan::Backspaces(5)));
        assert!(matches!(&committed.steps[2], Plan::Type(t) if !t.ends_with(' ')));
    }

    #[test]
    fn the_hotkey_fixes_a_single_character() {
        // "ก" is what the Thai layout makes of the "d" key: the typist meant "d"
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        live.mode = Mode::Manual;
        live.on_char('ก');

        let committed = live.on_fix_hotkey(1).expect("one character is a fix too");
        assert_eq!(committed.shown, "ก");
        assert_eq!(committed.corrected, "d");
        assert_eq!(committed.steps.len(), 3);
        assert!(matches!(committed.steps[0], Plan::Switch(InputLayout::UsQwerty)));
        // the application's own Backspace already took the "ก": deleting again
        // would eat the character before it
        assert!(matches!(committed.steps[1], Plan::Backspaces(0)));
        assert!(matches!(committed.steps[2], Plan::Type(ref t) if t == "d"));
    }

    #[test]
    fn the_hotkey_fixes_a_single_latin_character_the_other_way() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        live.on_char('d');

        let committed = live.on_fix_hotkey(1).expect("one character is a fix too");
        assert_eq!(committed.corrected, "ก");
        assert!(matches!(committed.steps[0], Plan::Switch(InputLayout::ThaiKedmanee)));
        assert!(matches!(committed.steps[1], Plan::Backspaces(0)));
    }

    #[test]
    fn a_word_typed_correctly_is_not_flipped_even_by_the_hotkey() {
        // a real word on screen stays put: an accidental press must not mangle it
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        live.mode = Mode::Manual;
        for c in "สวัสดี".chars() {
            live.on_char(c);
        }
        assert!(live.on_fix_hotkey(1).is_none());
        assert_eq!(live.skipped, 1);
    }

    #[test]
    fn the_reported_pairs_flip_on_a_deliberate_press() {
        // Suvasin's report: "กก" and "gg" could not be fixed, because for two
        // characters both dictionaries have something to say and neither verdict
        // means anything
        for (typed, expected, layout) in [
            ("dd", "กก", InputLayout::UsQwerty),
            ("กก", "dd", InputLayout::ThaiKedmanee),
            ("gg", "เเ", InputLayout::UsQwerty),
            ("เเ", "gg", InputLayout::ThaiKedmanee),
        ] {
            let mut live = Live::new(layout);
            live.mode = Mode::Manual;
            for c in typed.chars() {
                live.on_char(c);
            }
            let committed = live
                .on_fix_hotkey(1)
                .unwrap_or_else(|| panic!("{typed:?} should be fixable"));
            assert_eq!(committed.shown, typed);
            assert_eq!(committed.corrected, expected);
        }
    }

    #[test]
    fn a_long_string_no_dictionary_knows_is_read_the_other_way() {
        // a name or a loanword typed in the wrong layout is not in any dictionary
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        live.mode = Mode::Manual;
        let typed = read_in_other("Suvasin", InputLayout::UsQwerty);
        assert_eq!(typed.chars().count(), 7);
        for c in typed.chars() {
            live.on_char(c);
        }
        let committed = live.on_fix_hotkey(1).expect("not a word, so it flips");
        assert_eq!(committed.corrected, "Suvasin");
    }

    #[test]
    fn both_mode_fixes_on_a_boundary_and_the_hotkey_swaps_it_back() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Both;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        let fixed = live.on_boundary(' ').expect("it fixes on the boundary like auto");
        assert_eq!(fixed.corrected, "ดอกไม้");
        assert!(matches!(fixed.steps[1], Plan::Backspaces(7)));

        // the hotkey puts the "wrong" spelling back
        let back = live.on_fix_hotkey(1).expect("the hotkey swaps it back");
        assert_eq!(back.shown, "ดอกไม้");
        assert_eq!(back.corrected, "fvdw,h");
        assert!(matches!(back.steps[0], Plan::Switch(InputLayout::UsQwerty)));
        assert!(matches!(back.steps[1], Plan::Backspaces(6)));
        assert!(matches!(back.steps[2], Plan::Type(ref t) if t == "fvdw,h "));

        // and again: it is a toggle, not a one-way undo
        let again = live.on_fix_hotkey(1).expect("a second press swaps it forward");
        assert_eq!(again.shown, "fvdw,h");
        assert_eq!(again.corrected, "ดอกไม้");
        assert!(matches!(again.steps[1], Plan::Backspaces(6)));
        assert!(matches!(again.steps[2], Plan::Type(ref t) if t == "ดอกไม้ "));
    }

    #[test]
    fn both_mode_still_fixes_a_word_that_was_typed_since() {
        // a new word means the press is about *that* word, not about the swap
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Both;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_some());
        // ...and the next word is typed with the layout the fix left behind (Thai)
        for c in "แนพพำแะ".chars() {
            live.on_char(c);
        }
        let fixed = live.on_fix_hotkey(1).expect("a normal fix, mid-word");
        assert_eq!(fixed.corrected, "correct");
        // seven characters on screen, one already eaten by the hotkey's Backspace
        assert!(matches!(fixed.steps[1], Plan::Backspaces(6)));
    }

    #[test]
    fn auto_and_manual_are_untouched_by_the_new_mode() {
        let mut auto = Live::new(InputLayout::UsQwerty);
        auto.mode = Mode::Auto;
        for c in "fvdw,h".chars() {
            auto.on_char(c);
        }
        assert_eq!(auto.on_boundary(' ').unwrap().corrected, "ดอกไม้");

        let mut manual = Live::new(InputLayout::UsQwerty);
        manual.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            manual.on_char(c);
        }
        assert!(manual.on_boundary(' ').is_none(), "manual waits for the hotkey");
        assert_eq!(manual.on_fix_hotkey(1).unwrap().corrected, "ดอกไม้");
    }

    #[test]
    fn the_history_keeps_the_newest_fix_first_and_can_be_undone() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Both;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_some());
        assert!(live.can_undo(), "the fix can still be undone");
        assert_eq!(live.history.len(), 1);
        assert_eq!(live.history[0].shown, "fvdw,h");
        assert_eq!(live.history[0].corrected, "ดอกไม้");
        assert_eq!(live.history[0].kind, "fix");

        let undone = live.undo_last().expect("the button undoes it");
        assert_eq!(undone.corrected, "fvdw,h");
        assert_eq!(live.history.len(), 2);
        assert_eq!(live.history[0].kind, "swap");
        assert!(live.can_undo(), "and it is undoable again, as a toggle");
    }

    #[test]
    fn the_history_forgets_old_entries() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Auto;
        for _ in 0..(HISTORY + 5) {
            // a fix switches the layout, so the typist switches back before the
            // next word - exactly what happens in real life
            live.note_user_switch(InputLayout::UsQwerty);
            for c in "fvdw,h".chars() {
                live.on_char(c);
            }
            assert!(live.on_boundary(' ').is_some());
        }
        assert_eq!(live.history.len(), HISTORY);
    }

    #[test]
    fn auto_now_rescues_short_words_without_being_asked() {
        // the word checker refuses thousands of 3-character words because the
        // other reading is ambiguous; the second opinion fixes them, and a
        // single character is still never touched without a press
        let mut rescued = None;
        for word in crate::dict::thai_words().filter(|w| w.chars().count() == 3) {
            let typed = read_in_other(word, InputLayout::ThaiKedmanee);
            if typed == word {
                continue;
            }
            let checked = crate::policy::detect_token(
                &typed,
                InputLayout::UsQwerty,
                crate::dict::english(),
                crate::dict::thai(),
            );
            if checked.is_some() || careful_reading(&typed, InputLayout::UsQwerty).is_none() {
                continue;
            }
            rescued = Some((typed, word.to_string()));
            break;
        }
        let (typed, word) = rescued.expect("a short word that needs the second opinion");
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Auto;
        for c in typed.chars() {
            live.on_char(c);
        }
        assert_eq!(
            live.on_boundary(' ').map(|c| c.corrected),
            Some(word),
            "auto fixes {typed:?} now"
        );

        let mut single = Live::new(InputLayout::UsQwerty);
        single.mode = Mode::Auto;
        single.on_char('d');
        assert!(single.on_boundary(' ').is_none(), "one character is never guessed");
    }

    #[test]
    fn the_space_chord_deletes_the_word_and_the_space_it_typed() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        let committed = live
            .on_fix_hotkey_via_space()
            .expect("the space chord fixes the word");
        assert_eq!(committed.shown, "fvdw,h");
        assert!(matches!(committed.steps[0], Plan::Switch(InputLayout::ThaiKedmanee)));
        // six characters plus the space the application already inserted
        assert!(matches!(committed.steps[1], Plan::Backspaces(7)));
        // and no trailing space: the space was a keypress, not a boundary
        assert!(matches!(committed.steps[2], Plan::Type(ref t) if t == "ดอกไม้"));
    }

    #[test]
    fn the_space_chord_works_on_one_character_too() {
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        live.mode = Mode::Manual;
        live.on_char('ก');
        let committed = live.on_fix_hotkey_via_space().expect("a single character");
        assert_eq!(committed.corrected, "d");
        assert!(matches!(committed.steps[1], Plan::Backspaces(2))); // "ก" and the space
    }

    #[test]
    fn the_hotkey_rescues_a_short_word_the_word_checker_refuses() {
        // Short tokens are where the word checker gives up, and thousands of real
        // three-character Thai words are in that set. Find one and prove the
        // hotkey still fixes it.
        for word in crate::dict::thai_words().filter(|w| w.chars().count() == 3) {
            let typed = read_in_other(word, InputLayout::ThaiKedmanee);
            if typed == word {
                continue;
            }
            let checked = crate::policy::detect_token(
                &typed,
                InputLayout::UsQwerty,
                crate::dict::english(),
                crate::dict::thai(),
            );
            if checked.is_some() || careful_reading(&typed, InputLayout::UsQwerty).is_none() {
                continue; // not the case this test is about
            }
            let mut live = Live::new(InputLayout::UsQwerty);
            live.mode = Mode::Manual;
            for c in typed.chars() {
                live.on_char(c);
            }
            let committed = live.on_fix_hotkey(1).expect("the hotkey rescues it");
            assert_eq!(committed.corrected, word);
            return;
        }
        panic!("no short Thai word that needs the deliberate-request reading - test is stale");
    }

    #[test]
    fn a_single_character_the_other_layout_reads_the_same_is_left_alone() {
        // "7" is "7" in both layouts: nothing to fix, and nothing to delete
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        live.on_char('7');
        assert!(live.on_fix_hotkey(1).is_none());
        assert_eq!(live.skipped, 1);
    }

    #[test]
    fn a_single_character_works_through_the_reversed_chord_too() {
        let mut live = Live::new(InputLayout::ThaiKedmanee);
        live.mode = Mode::Manual;
        live.on_char('ก');
        live.note_backspace();
        live.on_backspace();

        let committed = live
            .on_fix_hotkey_after_backspace()
            .expect("the reversed chord fixes one character too");
        assert_eq!(committed.corrected, "d");
        assert!(matches!(committed.steps[1], Plan::Backspaces(0)));
    }

    #[test]
    fn the_hotkey_works_when_backspace_arrives_before_shift() {
        // a fast chord can deliver the keys in either order; the fix must not
        // depend on which one the kernel reports first
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        live.note_backspace(); // the Backspace landed first
        live.on_backspace(); // ...and the application ate the "h"

        let committed = live
            .on_fix_hotkey_after_backspace()
            .expect("the reversed chord still fixes the word");
        assert_eq!(committed.shown, "fvdw,h");
        assert_eq!(layout_of(&committed.corrected), InputLayout::ThaiKedmanee);
        assert!(matches!(committed.steps[1], Plan::Backspaces(5)));
    }

    #[test]
    fn an_empty_word_produces_no_plan() {
        let mut live = Live::new(InputLayout::UsQwerty);
        assert!(live.on_boundary(' ').is_none());
    }
}


/// The two recall windows: a word that is already on screen is reachable for
/// [`config::DEFAULT_RECALL_SECONDS`], the newest fix for
/// [`config::DEFAULT_UNDO_SECONDS`], and what the daemon can see happening
/// (Escape, an arrow key, a layout switch) forgets both at once.
#[cfg(test)]
mod recall_window_tests {
    use super::*;
    use crate::linuxos::config::{window_ms, DEFAULT_RECALL_SECONDS, DEFAULT_UNDO_SECONDS};

    const RECALL_MS: u64 = window_ms(DEFAULT_RECALL_SECONDS);
    const UNDO_MS: u64 = window_ms(DEFAULT_UNDO_SECONDS);

    /// A word the space closed: exactly what the fix chord reaches for.
    fn word_then_space(mode: Mode) -> Live {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = mode;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        live.on_boundary(' ');
        live
    }

    #[test]
    fn a_word_inside_the_recall_window_is_still_fixed() {
        let mut live = word_then_space(Mode::Manual);
        live.set_now(RECALL_MS);
        let committed = live.on_fix_hotkey(1).expect("15 s is still inside the window");
        assert_eq!(committed.shown, "fvdw,h");
        assert_eq!(live.skipped, 0);
    }

    #[test]
    fn a_word_past_the_recall_window_is_forgotten() {
        let mut live = word_then_space(Mode::Manual);
        live.set_now(RECALL_MS + 1);
        assert!(live.on_fix_hotkey(1).is_none(), "a 15 s old word may sit anywhere");
        assert_eq!(live.skipped, 1, "the press is counted as skipped, not as a fix");
    }

    #[test]
    fn the_space_chord_forgets_it_too() {
        let mut live = word_then_space(Mode::Manual);
        live.set_now(RECALL_MS + 1);
        assert!(live.on_fix_hotkey_via_space().is_none());
        assert_eq!(live.skipped, 1);
    }

    #[test]
    fn the_window_is_measured_from_the_boundary_that_closed_the_word() {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        live.on_boundary(' ');
        live.set_now(RECALL_MS - 1);
        assert!(live.on_fix_hotkey(1).is_some(), "one millisecond short of the edge");
    }

    #[test]
    fn an_escape_forgets_the_word_at_once() {
        let mut live = word_then_space(Mode::Manual);
        live.on_reset(); // what Escape, an arrow or Home/End does in the daemon
        live.set_now(1);
        assert!(live.on_fix_hotkey(1).is_none(), "the cursor moved: nothing to fix");
        assert_eq!(live.skipped, 1);
    }

    #[test]
    fn a_deliberate_layout_switch_forgets_the_word_too() {
        let mut live = word_then_space(Mode::Manual);
        live.note_user_switch(InputLayout::ThaiKedmanee);
        live.set_now(1);
        assert!(live.on_fix_hotkey(1).is_none());
    }

    #[test]
    fn the_word_being_typed_never_expires() {
        // mid-word the buffer is anchored to the cursor, so age cannot matter
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = Mode::Manual;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        live.set_now(RECALL_MS * 10);
        assert!(live.on_fix_hotkey(1).is_some(), "still under the cursor");
    }

    /// Auto mode fixes at the boundary but keeps no swap (only `both` does), so
    /// the undo window is exercised where the button exists: `both` and manual.
    fn auto_fixed(mode: Mode) -> Live {
        let mut live = Live::new(InputLayout::UsQwerty);
        live.mode = mode;
        for c in "fvdw,h".chars() {
            live.on_char(c);
        }
        assert!(live.on_boundary(' ').is_some(), "the automatic fix happened");
        live
    }

    #[test]
    fn undo_outlives_the_recall_window_and_then_ends() {
        let mut live = auto_fixed(Mode::Both);
        live.set_now(RECALL_MS * 3);
        assert!(live.can_undo(), "the button and the guard agree");
        assert!(live.undo_last().is_some(), "inside the undo window");

        let mut stale = auto_fixed(Mode::Both);
        stale.set_now(UNDO_MS + 1);
        assert!(!stale.can_undo(), "the button must not lie");
        assert!(stale.undo_last().is_none());
        assert!(stale.on_fix_hotkey(1).is_none(), "and the old word is forgotten");
    }

    #[test]
    fn undo_available_at_the_very_edge_of_its_window() {
        let mut live = auto_fixed(Mode::Both);
        live.set_now(UNDO_MS);
        assert!(live.can_undo());
        assert!(live.undo_last().is_some());
    }

    #[test]
    fn a_fix_made_by_the_chord_is_undoable_inside_its_window() {
        let mut live = word_then_space(Mode::Manual);
        assert!(live.on_fix_hotkey(1).is_some(), "the chord fixed the word");
        live.set_now(UNDO_MS);
        assert!(live.can_undo(), "manual fixes keep a swap too");
    }
}


/// The repeated press, against a stand-in for the focused text box.
///
/// The application's *own* keystrokes arrive before the daemon decides - the
/// chord's Backspace, and every character the typist types - so the plans have to
/// add up to exactly what is on screen. They did not: each swap deleted one
/// character too many when no boundary had been typed, which ate the text to the
/// left of the word on every press after the first.
#[cfg(test)]
mod chord_repeat_tests {
    use super::*;

    /// "fvdw,h" on the English layout, and what it was meant to be.
    const WRONG: &str = "fvdw,h";
    const RIGHT: &str = "\u{e14}\u{e2d}\u{e01}\u{e44}\u{e21}\u{e49}";
    /// "หวัดดี" - text that already sits to the left of the word.
    const BEFORE: &str = "\u{e2b}\u{e31}\u{e14}\u{e14}\u{e35} ";

    struct Screen {
        live: Live,
        text: String,
    }

    impl Screen {
        fn new(mode: Mode, before: &str) -> Self {
            let mut live = Live::new(InputLayout::UsQwerty);
            live.mode = mode;
            Screen {
                live,
                text: before.to_string(),
            }
        }

        fn type_chars(&mut self, text: &str) {
            for c in text.chars() {
                self.live.on_char(c);
                self.text.push(c);
            }
        }

        fn boundary(&mut self, boundary: char) {
            self.text.push(boundary);
            if let Some(committed) = self.live.on_boundary(boundary) {
                self.apply(&committed.steps);
            }
        }

        /// Shift+Backspace: the application deletes one character before we see it.
        fn chord(&mut self) {
            self.text.pop();
            if let Some(committed) = self.live.on_fix_hotkey(1) {
                self.apply(&committed.steps);
            }
        }

        /// The settings window's Undo button: no keystroke deleted anything first.
        fn tray_undo(&mut self) {
            if let Some(committed) = self.live.undo_last() {
                self.apply(&committed.steps);
            }
        }

        fn apply(&mut self, steps: &[Plan]) {
            for step in steps {
                match step {
                    Plan::Switch(_) => {}
                    Plan::Backspaces(n) => {
                        for _ in 0..*n {
                            self.text.pop();
                        }
                    }
                    Plan::Type(text) => self.text.push_str(text),
                }
            }
        }
    }

    #[test]
    fn the_chord_pressed_three_times_toggles_without_eating_its_neighbours() {
        // mid-word: no boundary was typed, so every press's own Backspace eats a
        // character of the word itself
        let mut screen = Screen::new(Mode::Both, "ab ");
        screen.type_chars(WRONG);
        for expected in [RIGHT, WRONG, RIGHT] {
            screen.chord();
            assert_eq!(
                screen.text,
                format!("ab {expected}"),
                "the text before the word has to stay put"
            );
        }
    }

    #[test]
    fn after_a_boundary_the_chord_toggles_the_same_way() {
        let mut screen = Screen::new(Mode::Both, "ab ");
        screen.type_chars(WRONG);
        screen.boundary(' '); // `both` fixes at the boundary
        assert_eq!(screen.text, format!("ab {RIGHT} "));
        for expected in [WRONG, RIGHT, WRONG] {
            screen.chord();
            assert_eq!(screen.text, format!("ab {expected} "));
        }
    }

    #[test]
    fn the_tray_undo_puts_the_word_back_exactly() {
        let mut screen = Screen::new(Mode::Both, "ab ");
        screen.type_chars(WRONG);
        screen.chord();
        assert_eq!(screen.text, format!("ab {RIGHT}"));
        screen.tray_undo();
        assert_eq!(screen.text, format!("ab {WRONG}"));
        screen.tray_undo();
        assert_eq!(screen.text, format!("ab {RIGHT}"));
    }

    #[test]
    fn the_tray_undo_after_an_automatic_fix_keeps_the_boundary() {
        let mut screen = Screen::new(Mode::Both, "ab ");
        screen.type_chars(WRONG);
        screen.boundary(' ');
        assert_eq!(screen.text, format!("ab {RIGHT} "));
        screen.tray_undo();
        assert_eq!(
            screen.text,
            format!("ab {WRONG} "),
            "the boundary is neither doubled nor lost"
        );
    }

    #[test]
    fn ten_presses_in_a_row_leave_the_sentence_alone() {
        // the mangling that was reported: each press deleted one character more
        // than the word, chewing the text to the left of it
        let mut screen = Screen::new(Mode::Both, BEFORE);
        screen.type_chars(WRONG);
        for press in 0..10 {
            screen.chord();
            assert!(
                screen.text == format!("{BEFORE}{WRONG}")
                    || screen.text == format!("{BEFORE}{RIGHT}"),
                "press {press} left the screen as {:?}",
                screen.text
            );
        }
    }
}
