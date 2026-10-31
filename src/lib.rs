//! RightType — a modern, safe Rust successor to RightLang.
//!
//! Fixes text typed in the wrong keyboard layout (e.g. Thai Kedmanee typed while
//! the English QWERTY layout is active, or vice-versa) without retyping.
//!
//! The crate is split into an OS-free **pure-logic core** (this library) and a
//! Windows **integration layer** (the binary). Keeping the core OS-free makes it
//! exhaustively unit-testable and lets a future macOS backend reuse it unchanged.
//!
//! Pure-logic modules:
//! - [`layout`] — N-layout-generic conversion engine (Kedmanee ↔ QWERTY in v1).
//! - [`secret`] — hard-deny key/address shapes plus password/BIP39 stream guards.
//! - [`dict`] — word dictionaries for membership checks.
//! - [`english`] — English beyond the dictionary: compounds and continuations.
//! - [`hotkeys`] — the hotkeys, how they are written, and which are safe.
//! - [`i18n`] — interface strings in English and Thai.
//! - [`predict`] — per-field language habits (opt-in) for switching before typing.
//! - [`per_app`] — per-app correction modes and their text form.
//! - [`detect`] — layout-mismatch detection over completed words.
//! - [`diag`] — recent decisions, with no typed text, for a problem report.
//! - [`code`] — Code mode: what to fix in a code editor.
//! - [`buffer`] — current-word input buffer (zeroized on every word boundary).
//! - [`segment`] — Thai word segmentation (maximal matching) for space-less Thai.
//! - [`motion`] — how the overlay pill fades and rises in and out.
//! - [`render`] — reconciling on-screen text with the run's current best reading.
//! - [`repair`] — fixing a whole piece of finished text (the Fix text window).
//! - [`recent`] — the last few completed words, for flipping back several at once.
//! - [`usage`] — daily correction counts for the opt-in weekly view.
//! - [`spelling`] — common Thai misspellings, put right (opt-in).
//! - [`snippets`] — a short trigger becomes a longer text, on either keyboard.
//! - [`sim`] — the hook's Auto pipeline replayed on a string, for benchmarks and tests.

pub mod abbrev;
pub mod accents;
pub mod app_quality;
pub mod atcaret;
pub mod breaks;
pub mod buffer;
pub mod chars;
pub mod charsets;
pub mod chatter;
pub mod code;
pub mod compat;
pub mod detect;
pub mod diag;
pub mod dict;
pub mod english;
pub mod find;
pub mod ghost;
pub mod hotkeys;
pub mod i18n;
pub mod keyboard;
pub mod keycast;
pub mod latex;
pub mod layout;
pub mod macros;
pub mod mathsimple;
pub mod motion;
pub mod naturalmath;
pub mod notice;
pub mod per_app;
pub mod policy;
pub mod predict;
pub mod recent;
pub mod render;
pub mod repair;
pub mod secret;
pub mod segment;
pub mod shortcuts;
pub mod sim;
pub mod snippets;
pub mod spelling;
pub mod summon;
pub mod thai_text;
pub mod timing;
pub mod trainer;
pub mod usage;
pub mod why;

/// The Linux desktop layer: evdev capture, uinput injection, layout queries.
/// Windows code lives in the platform crate's own modules gated on `winos`.
#[cfg(all(target_os = "linux", feature = "linuxos"))]
pub mod linuxos;
