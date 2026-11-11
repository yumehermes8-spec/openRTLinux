//! The typist's own words: what to keep as typed, and what to fix by hand.
//!
//! The bundled dictionaries are conversational and miss domain vocabulary - a
//! developer's `kubernetes`, a product name, a friend's nickname typed in Thai.
//! One file holds the two lists that fix that, and the settings window edits the
//! same file the daemon reads:
//!
//! ```text
//! learn = true
//! word = kubernetes
//! word = สมชาย
//! pair = frdw,h=ดอกไม้
//! ```
//!
//! * `word` - never touch this one. Added by hand, by [`Words::keep_as_typed`]
//!   (the typist reversed one of our fixes, which is an explicit "that was
//!   right"), or by [`Words::observe`] after the same English word shows up
//!   [`REPEATS`] times. The words join the live dictionaries
//!   ([`Dictionary::learn`]), so every decision sees them: the automatic path,
//!   the fix chord, the second opinion - no other change needed.
//! * `pair` - `on screen=what it should be`, for the slips this typist actually
//!   makes. A pair beats every guess we could make: it is applied at a boundary
//!   and on the fix chord, whatever the layouts say.
//! * `learn = true|false` - the automatic half only. Words and pairs added by
//!   hand stay either way.
//!
//! A repeated-key line format, not TOML: both sides write it (the daemon learns,
//! the settings window edits), Thai has no escaping rules to get wrong, and a
//! half-written line costs one word instead of the file.
//!
//! [`Dictionary::learn`]: crate::dict::Dictionary::learn

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::code::looks_like_identifier;
use crate::dict;

/// The same English word this many times before it is worth keeping.
pub const REPEATS: u8 = 3;
const MIN_LEN: usize = 3;
const MAX_LEN: usize = 20;
const MIN_THAI_LEN: usize = 2;
const MAX_THAI_LEN: usize = 30;

/// Where the lists live, next to `config.toml`.
pub fn path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        format!("{}/.config", std::env::var("HOME").unwrap_or_else(|_| ".".into()))
    });
    Path::new(&base).join("openrtlinux").join("words.toml")
}

#[derive(Debug, Default, Clone)]
pub struct Words {
    /// Learn English words the typist keeps typing. Off = only what was added
    /// by hand (or kept as typed) is remembered.
    pub learn: bool,
    /// Never touch these.
    words: BTreeSet<String>,
    /// `on screen` -> `what it should be`.
    pairs: BTreeMap<String, String>,
    /// Sightings on the way to a learned word. Not persisted.
    pending: BTreeMap<String, u8>,
}

impl Words {
    /// Read the file. A missing or unreadable file is an empty list, never an
    /// error: the daemon has to start either way.
    pub fn load(path: &Path) -> Self {
        let mut words = Words::default();
        let Ok(text) = std::fs::read_to_string(path) else {
            return words;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "learn" => words.learn = matches!(value, "true" | "1" | "yes" | "on"),
                "word" => {
                    if !value.is_empty() {
                        words.words.insert(value.to_string());
                    }
                }
                "pair" => {
                    if let Some((wrong, right)) = value.split_once('=') {
                        let (wrong, right) = (wrong.trim(), right.trim());
                        if !wrong.is_empty() && !right.is_empty() {
                            words.pairs.insert(wrong.to_string(), right.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        words
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = String::from(
            "# openRTLinux - the typist's own words. Written by the settings window\n\
             # and by the daemon when it learns one. `word = never fix this`,\n\
             # `pair = what I typed=what it should be`.\n",
        );
        out.push_str(&format!("learn = {}\n", self.learn));
        for word in &self.words {
            out.push_str(&format!("word = {word}\n"));
        }
        for (wrong, right) in &self.pairs {
            out.push_str(&format!("pair = {wrong}={right}\n"));
        }
        std::fs::write(path, out)
    }

    /// Push every word into the live dictionary of its script, so the decisions
    /// see them. Replaces whatever was learned before (the file is the truth).
    pub fn teach_dictionaries(&self) {
        dict::english().forget_learned();
        dict::thai().forget_learned();
        for word in &self.words {
            teach(word);
        }
    }

    /// What the typist said this string should be, if they said anything.
    pub fn resolve(&self, shown: &str) -> Option<String> {
        self.pairs.get(shown).cloned()
    }

    /// Is this word one we must leave alone?
    pub fn protects(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    pub fn words(&self) -> impl Iterator<Item = &String> {
        self.words.iter()
    }

    pub fn pairs(&self) -> impl Iterator<Item = (&String, &String)> {
        self.pairs.iter()
    }

    /// Add a word by hand. `true` when it was not there already.
    pub fn add(&mut self, word: &str) -> bool {
        let word = tidy(word);
        if word.is_empty() {
            return false;
        }
        self.pending.remove(&word);
        self.words.insert(word)
    }

    pub fn remove(&mut self, word: &str) -> bool {
        self.words.remove(word.trim())
    }

    /// `on screen` -> `what it should be`.
    pub fn add_pair(&mut self, wrong: &str, right: &str) -> bool {
        let (wrong, right) = (tidy(wrong), tidy(right));
        if wrong.is_empty() || right.is_empty() {
            return false;
        }
        self.pairs.insert(wrong, right).is_none()
    }

    pub fn remove_pair(&mut self, wrong: &str) -> bool {
        self.pairs.remove(wrong.trim()).is_some()
    }

    /// "That was right as I typed it" - the typist reversed one of our fixes.
    /// Learned even with the automatic half off, because it is not a guess.
    ///
    /// Still shape-checked, and looser than the automatic half: a swap can fire
    /// while a key is stuck or a word is half-typed, and a list that slowly fills
    /// with junk is a list nobody trusts (the first `vvdde]y'` arrived that way,
    /// because the `]` and `'` were still keys the typist meant to press). Two
    /// letters are enough: `ok` is a word a person can decide to keep.
    pub fn keep_as_typed(&mut self, word: &str) -> bool {
        let word = tidy(word);
        if keepable(&word) {
            return self.add(&word);
        }
        false
    }

    /// One sighting of a finished English word. After [`REPEATS`] of them it is
    /// kept. Returns `true` when this sighting added the word.
    pub fn observe(&mut self, word: &str) -> bool {
        if !self.learn {
            return false;
        }
        let word = tidy(word);
        if !eligible_english(&word) || dict::english().contains(&word) {
            return false;
        }
        let count = self.pending.entry(word.clone()).or_insert(0);
        *count += 1;
        if *count < REPEATS {
            return false;
        }
        self.pending.remove(&word);
        self.words.insert(word)
    }

    /// Forget everything learned automatically; the words added by hand stay.
    pub fn clear_learned(&mut self) -> usize {
        let bundled: BTreeSet<String> = self.words.iter().filter(|w| is_bundled(w)).cloned().collect();
        let before = self.words.len();
        self.words = bundled;
        self.pending.clear();
        before - self.words.len()
    }
}

/// The file's words, minus the ones the bundled dictionaries already know.
fn is_bundled(word: &str) -> bool {
    dict::english().contains(word) || dict::thai().contains(word)
}

/// One word into the right dictionary.
fn teach(word: &str) -> bool {
    if word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        if eligible_thai(word) {
            dict::thai().learn(word);
            return true;
        }
    } else if eligible_english(word) {
        dict::english().learn(word);
        return true;
    }
    false
}

/// Trim what travels with a token but is not part of the word.
fn tidy(word: &str) -> String {
    word.trim()
        .trim_matches(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .to_string()
}

/// English worth keeping: letters (and the odd hyphen or apostrophe), the length
/// a word can be, not ALL CAPS, and not shaped like code - a name or an
/// identifier is not something a dictionary should be taught.
pub fn eligible_english(word: &str) -> bool {
    let len = word.chars().count();
    if !(MIN_LEN..=MAX_LEN).contains(&len) {
        return false;
    }
    if !word.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'') {
        return false;
    }
    if word.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    !looks_like_identifier(word)
}

/// Worth keeping when a person just said so: letters of one script (plus the
/// hyphen and apostrophe English words carry), the length a word can be, not ALL
/// CAPS, not shaped like code.
pub fn keepable(word: &str) -> bool {
    let len = word.chars().count();
    if !(2..=MAX_THAI_LEN).contains(&len) {
        return false;
    }
    if word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        return eligible_thai(word);
    }
    if !word.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'') {
        return false;
    }
    if word.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    !looks_like_identifier(word)
}

/// Thai worth keeping: Thai letters, the length a Thai word can be.
pub fn eligible_thai(word: &str) -> bool {
    let len = word.chars().count();
    if !(MIN_THAI_LEN..=MAX_THAI_LEN).contains(&len) {
        return false;
    }
    word.chars().all(|c| ('\u{0E01}'..='\u{0E5B}').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openrtlinux-words-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("words.toml")
    }

    #[test]
    fn a_file_round_trips() {
        let path = temp("round-trip");
        let mut words = Words::default();
        words.learn = true;
        assert!(words.add("kubernetes"));
        assert!(words.add("\u{0e2a}\u{0e21}\u{0e0a}\u{0e32}\u{0e22}"));
        assert!(words.add_pair("frdw,h", "\u{0e14}\u{0e2d}\u{0e01}\u{0e44}\u{0e21}\u{0e49}"));
        words.save(&path).expect("saved");

        let back = Words::load(&path);
        assert!(back.learn);
        assert!(back.protects("kubernetes"));
        assert!(back.protects("\u{0e2a}\u{0e21}\u{0e0a}\u{0e32}\u{0e22}"));
        assert_eq!(
            back.resolve("frdw,h").as_deref(),
            Some("\u{0e14}\u{0e2d}\u{0e01}\u{0e44}\u{0e21}\u{0e49}")
        );
    }

    #[test]
    fn a_missing_file_is_an_empty_list_not_an_error() {
        let words = Words::load(Path::new("/nonexistent/openrtlinux/words.toml"));
        assert_eq!(words.words().count(), 0);
        assert!(!words.learn);
    }

    #[test]
    fn a_broken_line_costs_one_word() {
        let path = temp("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "learn = true\nword = kubernetes\nthis line is nonsense\npair = frdw,h\nword = tokio\n",
        )
        .unwrap();
        let words = Words::load(&path);
        assert!(words.protects("kubernetes") && words.protects("tokio"));
        assert_eq!(words.pairs().count(), 0, "a pair with no = is skipped");
    }

    #[test]
    fn an_english_word_is_learned_on_the_third_sighting() {
        // not a word the bundled lists know: those are never re-learned
        let mut words = Words::default();
        words.learn = true;
        assert!(!words.observe("notawordinthelist"));
        assert!(!words.observe("notawordinthelist"));
        assert!(words.observe("notawordinthelist"), "the third sighting keeps it");
        assert!(words.protects("notawordinthelist"));
    }

    #[test]
    fn a_word_the_bundled_tech_list_knows_is_never_learned_again() {
        let mut words = Words::default();
        words.learn = true;
        for _ in 0..REPEATS {
            assert!(!words.observe("kubernetes"), "already in assets/tech_terms.txt");
        }
        assert_eq!(words.words().count(), 0);
    }

    #[test]
    fn learning_off_means_no_automatic_words() {
        let mut words = Words::default();
        assert!(!words.observe("kubernetes"));
        assert!(!words.observe("kubernetes"));
        assert!(!words.observe("kubernetes"));
        assert_eq!(words.words().count(), 0);
    }

    #[test]
    fn words_the_bundled_lists_know_are_not_learned() {
        let mut words = Words::default();
        words.learn = true;
        for _ in 0..REPEATS {
            assert!(!words.observe("the"));
        }
        assert_eq!(words.words().count(), 0);
    }

    #[test]
    fn code_and_caps_and_numbers_are_not_words_to_keep() {
        assert!(!eligible_english("KUBERNETES"));
        assert!(!eligible_english("camelCase"));
        assert!(!eligible_english("snake_case"));
        assert!(!eligible_english("utf8"));
        assert!(!eligible_english("ab"));
        assert!(eligible_english("kubernetes"));
        assert!(eligible_english("well-known"));
    }

    #[test]
    fn keeping_a_word_as_typed_works_with_learning_off() {
        let mut words = Words::default();
        assert!(words.keep_as_typed("Suvasin"));
        assert!(words.protects("Suvasin"));
        assert!(words.keep_as_typed("ok"), "two letters, but the typist said so");
        assert!(words.keep_as_typed("\u{0e2d}\u{0e2d}\u{0e01}"));
    }

    #[test]
    fn junk_a_stuck_key_leaves_behind_is_not_kept() {
        // the first version of this feature remembered this one; a `]` and a `'`
        // are keys, not letters
        let mut words = Words::default();
        assert!(!words.keep_as_typed("vvdde]y'"));
        assert!(!words.keep_as_typed("MyClass"));
        assert!(!words.keep_as_typed("ALL CAPS"));
        assert!(!words.keep_as_typed("a"));
        assert!(!words.keep_as_typed(""));
        assert_eq!(words.words().count(), 0);
    }

    #[test]
    fn clearing_learned_keeps_what_was_added_by_hand() {
        let mut words = Words::default();
        words.learn = true;
        assert!(words.add("myproductname"));
        for _ in 0..REPEATS {
            words.observe("notawordinthelist");
        }
        assert!(words.protects("notawordinthelist"));
        // both look hand-added in memory; clearing drops what the dictionaries
        // do not know, which is the honest meaning of "forget learned"
        assert_eq!(words.clear_learned(), 2);
    }
}
