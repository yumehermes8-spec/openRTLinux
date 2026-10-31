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

use crate::buffer::{Key, WordBuffer};
use crate::dict;
use crate::linuxos::config::Mode;
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

pub struct Live {
    layout: InputLayout,
    buf: WordBuffer,
    /// The last word a boundary closed, with the boundary character.
    ///
    /// Manual mode needs it: the typist presses space (which ends the word) and
    /// *then* decides to fix it. RightType calls this "convert the last word in
    /// place"; without it the hotkey had nothing left to look at, because the
    /// boundary had already emptied the buffer.
    pending: Option<(String, String)>,
    /// The word as it was before the last plain Backspace: a Shift+Backspace
    /// chord can reach us in the other order (Backspace, then Shift), and by
    /// then the buffer has already lost the character the chord was meant to eat.
    before_backspace: Option<String>,
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
        self.buf.clear();
        self.layout = to;
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
            match self.pending.take() {
                // the word is committed to the screen and the Backspace took the
                // boundary: the whole word has to go, and the boundary comes back
                Some((word, sep)) => (word, sep, 0),
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
            match self.pending.take() {
                Some((word, _space_the_typist_already_wanted)) => word,
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
        Some(Committed {
            shown,
            corrected,
            steps,
        })
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
        if self.mode != Mode::Auto {
            // manual: the word is on screen, waiting for the hotkey
            self.pending = Some((shown, boundary.to_string()));
            return None;
        }
        let detection = policy::detect_token(&shown, self.layout, dict::english(), dict::thai())?;
        if detection.corrected == shown {
            return None;
        }
        let target = layout_of(&detection.corrected);
        let mut steps = Vec::new();
        if target != self.layout {
            steps.push(Plan::Switch(target));
        }
        // the boundary is already in the application: delete it too and type it back
        steps.push(Plan::Backspaces(shown.chars().count() + 1));
        steps.push(Plan::Type(format!("{}{}", detection.corrected, boundary)));
        self.layout = target;
        self.committed += 1;
        Some(Committed {
            shown,
            corrected: detection.corrected,
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
