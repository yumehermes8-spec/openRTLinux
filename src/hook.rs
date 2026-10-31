//! Low-level keyboard hook (`WH_KEYBOARD_LL`) — Windows only.
//!
//! This is the whole input→correct pipeline, run **synchronously on the hook
//! thread**. Keys pass through normally while we accumulate the current word; in
//! Auto mode a boundary that completes a wrong-layout word is **swallowed** and we
//! inject `backspaces + correction + boundary` in one atomic batch. Doing it
//! synchronously and swallowing the trigger makes the replacement race-free: the
//! mistyped letters are already in the target app before we inject, and because
//! every keystroke is serialised through this one thread, nothing can interleave —
//! even under fast typing. (An earlier async-worker design raced and garbled.)
//!
//! Modifier state (Shift/Ctrl/Alt/Caps) is read live from `GetAsyncKeyState` /
//! `GetKeyState` rather than tracked from the event stream. Tracking it ourselves
//! let a modifier *stick* whenever a key-up was missed — e.g. the Alt+Shift used to
//! switch keyboard layout — which then captured every following letter as if Shift
//! were held. Reading the real key state each time makes sticking impossible.
//!
//! The work here is tiny (a `ToUnicodeEx` call + two hashed dictionary lookups +
//! a small `SendInput`), so the callback stays far under the `LowLevelHooksTimeout`
//! that evicts slow hooks — see `docs/archive/PLAN.md`, Bug 1. We also ignore our own
//! injected events (tagged in `dwExtraInfo`, plus `LLKHF_INJECTED`) so a
//! correction can never feed back into itself.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use zeroize::Zeroize;

use righttype::buffer::{Key, WordBuffer};
use righttype::diag::{self, Shape};
use righttype::hotkeys::{Action, Chord, Hotkeys};
use righttype::layout::{auto_convert_with_layout, LayoutId};
use righttype::per_app::AppMode;
use righttype::recent::Recent;
use righttype::render;
use righttype::{dict, policy, secret};

use std::ffi::c_void;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(debug_assertions)]
use windows::Win32::UI::Input::KeyboardAndMouse::VK_PACKET;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, GetKeyboardLayoutList, ToUnicodeEx, HKL,
    VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME,
    VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, GA_ROOT, GUITHREADINFO, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN,
};

use crate::{inject, manual, safety};

/// Magic value stamped into `dwExtraInfo` on every event the injector sends, so
/// the hook can recognise and skip our own input with zero timing dependency. The
/// flag travels *with* the event, unlike a shared "injecting" boolean. ("RTYP")
pub const INJECT_TAG: usize = 0x5254_5950;

/// Set while we inject, as a secondary guard. The tag above is authoritative.
pub static INJECTING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Manual = 0,
    Auto = 1,
    Suggest = 2,
    /// Per-app only: code editors (see `righttype::code`).
    Code = 3,
}

impl Mode {
    fn next(self) -> Self {
        match self {
            Self::Manual => Self::Auto,
            Self::Auto => Self::Suggest,
            Self::Suggest | Self::Code => Self::Manual,
        }
    }

    /// Toast text announcing this mode, in the interface language.
    pub fn label(self) -> &'static str {
        use righttype::i18n::{tr, T};
        tr(match self {
            Self::Manual => T::ToastModeManual,
            Self::Auto => T::ToastModeAuto,
            Self::Suggest => T::ToastModeSuggest,
            Self::Code => T::ToastModeCode,
        })
    }
}

static MODE: AtomicU8 = AtomicU8::new(Mode::Manual as u8);

/// The key last pressed and not yet released (a further key-down of it is an
/// auto-repeat).
static HELD_KEY: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// The hotkeys in use (Settings → Hotkeys).
static HOTKEYS: std::sync::RwLock<Option<Hotkeys>> = std::sync::RwLock::new(None);

pub fn hotkeys() -> Hotkeys {
    HOTKEYS.read().unwrap().clone().unwrap_or_default()
}

pub fn set_hotkeys(hotkeys: Hotkeys) {
    *HOTKEYS.write().unwrap() = Some(hotkeys);
}

/// Settings is waiting for the next chord for this action; the result is
/// posted to `hwnd` as [`WM_HOTKEY_CAPTURED`].
static CAPTURE: Mutex<Option<(Action, isize)>> = Mutex::new(None);
/// The captured chord (`None` inside = cancelled with Esc).
static CAPTURED: Mutex<Option<(Action, Option<Chord>)>> = Mutex::new(None);
pub const WM_HOTKEY_CAPTURED: u32 = 0x8000 + 0x530;

/// Capture the next chord pressed anywhere as the new hotkey for `action`.
pub fn begin_capture(action: Action, hwnd: isize) {
    *CAPTURE.lock().unwrap() = Some((action, hwnd));
}

pub fn cancel_capture() {
    CAPTURE.lock().unwrap().take();
}

/// The chord captured for Settings, once it has been posted.
pub fn take_captured() -> Option<(Action, Option<Chord>)> {
    CAPTURED.lock().unwrap().take()
}

/// While Settings captures a hotkey: the first non-modifier key (CapsLock
/// counts) with the modifiers held is the chord; Esc cancels. The key is
/// swallowed either way.
unsafe fn capture_key(vk: u16) -> bool {
    let Some((action, hwnd)) = *CAPTURE.lock().unwrap() else {
        return false;
    };
    if is_modifier(vk) && vk != VK_CAPITAL.0 {
        return false;
    }
    CAPTURE.lock().unwrap().take();
    let chord = (vk != VK_ESCAPE.0)
        .then(|| Chord::new(is_down(VK_CONTROL), is_down(VK_SHIFT), is_down(VK_MENU), vk));
    *CAPTURED.lock().unwrap() = Some((action, chord));
    let _ = PostMessageW(
        HWND(hwnd as *mut c_void),
        WM_HOTKEY_CAPTURED,
        WPARAM(0),
        LPARAM(0),
    );
    true
}

/// Master on/off, controlled from the tray. When off the hook passes every key
/// straight through and touches nothing.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// CapsLock tapped on its own switches Thai/English (opt-in, from the
/// palette); held for half a second it toggles CapsLock as usual.
static CAPS_SWITCHES: AtomicBool = AtomicBool::new(false);
/// How long CapsLock must be held to act as CapsLock when it switches
/// languages.
const CAPS_HOLD: Duration = Duration::from_millis(500);

pub fn caps_switches_language() -> bool {
    CAPS_SWITCHES.load(Ordering::Relaxed)
}

pub fn set_caps_switches_language(on: bool) {
    CAPS_SWITCHES.store(on, Ordering::Relaxed);
}

thread_local! {
    /// When a CapsLock that switches languages went down (its key-down was
    /// swallowed; the release decides).
    static CAPS_DOWN_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// CapsLock released after its key-down was swallowed for switching: a tap
/// switches Thai/English, a hold toggles CapsLock.
unsafe fn caps_released(down_at: Instant) {
    if down_at.elapsed() >= CAPS_HOLD {
        inject::toggle_capslock();
        return;
    }
    let to = match policy::supported_layout_id(layout_id(effective_layout())) {
        Some(policy::InputLayout::ThaiKedmanee) => policy::InputLayout::UsQwerty,
        _ => policy::InputLayout::ThaiKedmanee,
    };
    activate_layout(to);
    if crate::caret::is_enabled() {
        crate::overlay::badge_at_caret(match to {
            policy::InputLayout::ThaiKedmanee => "ไทย",
            policy::InputLayout::UsQwerty => "ENG",
        });
    }
}

/// How many more automatic fixes this session show the Shift+Backspace tip.
static UNDO_TIPS_LEFT: AtomicU32 = AtomicU32::new(3);

/// Is RightType currently enabled?
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Enable or disable all correction.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.buf.clear();
            st.owned = None;
            st.mark = TokenMark::Plain;
            st.undo = None;
            st.recent.clear();
            st.suggestion = None;
            st.live_hint = None;
        });
    }
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Auto,
        2 => Mode::Suggest,
        _ => Mode::Manual,
    }
}

/// The mode that applies in the app being typed in: its own per-app mode, or
/// the global one. `None` when RightType is switched off in this app or in
/// this field.
fn mode_here() -> Option<Mode> {
    if crate::focus::field_is_off() {
        return None;
    }
    let own = STATE.with(|s| s.borrow().app_exe.as_deref().and_then(crate::apps::lookup));
    match own {
        Some(AppMode::Off) => None,
        Some(AppMode::Auto) => Some(Mode::Auto),
        Some(AppMode::Suggest) => Some(Mode::Suggest),
        Some(AppMode::Manual) => Some(Mode::Manual),
        Some(AppMode::Code) => Some(Mode::Code),
        None => Some(mode()),
    }
}

impl From<Mode> for AppMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Auto => AppMode::Auto,
            Mode::Suggest => AppMode::Suggest,
            Mode::Manual => AppMode::Manual,
            Mode::Code => AppMode::Code,
        }
    }
}

/// Ctrl+CapsLock: cycle the mode. In an app with its own mode that is the
/// mode cycled (so the hotkey does what it visibly does there); elsewhere the
/// global one.
unsafe fn cycle_mode() {
    use righttype::i18n::{tr, trf, T};
    let exe = safety::foreground_exe(GetForegroundWindow());
    let own = exe.as_deref().and_then(crate::apps::lookup);
    match (exe, own) {
        (Some(exe), Some(own)) => {
            let next = match own {
                AppMode::Manual => Mode::Auto,
                AppMode::Auto => Mode::Suggest,
                // Suggest, or Off (switched on again by the hotkey).
                _ => Mode::Manual,
            };
            crate::apps::set(&exe, Some(next.into()));
            crate::overlay::show(&trf(
                T::ToastAppMode,
                &[
                    (
                        "mode",
                        tr(match next {
                            Mode::Auto => T::ModeAuto,
                            Mode::Suggest => T::ModeSuggest,
                            Mode::Manual => T::ModeManual,
                            Mode::Code => T::ModeCode,
                        }),
                    ),
                    ("app", &exe),
                ],
            ));
            STATE.with(|s| s.borrow_mut().suggestion = None);
        }
        _ => {
            let next = mode().next();
            set_mode(next);
            crate::overlay::show(next.label());
        }
    }
    crate::config::persist_async();
}

pub fn set_mode(mode: Mode) {
    MODE.store(mode as u8, Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().suggestion = None);
}

thread_local! {
    /// Per-thread pipeline state. The hook callback always runs on the installing
    /// thread, so this persists across callbacks without locking.
    static STATE: RefCell<HookState> = RefCell::new(HookState::new());
}

struct HookState {
    buf: WordBuffer,
    seed: secret::SeedTracker,
    /// Foreground window + keyboard layout the buffer belongs to. When either
    /// changes — Alt-Tab, a click into another app, or a Thai/Eng layout switch —
    /// the buffered word no longer matches what's in front of the caret, so we
    /// drop it. This keeps the buffer honest across exactly the cases the user
    /// hit (switching layout mid-sentence left stale context behind).
    last_hwnd: isize,
    last_hkl: isize,
    /// Focus generation supplied by the UIA WinEvent hook. This catches focus
    /// changes between controls in the same top-level window.
    last_focus_generation: u64,
    /// When the context last changed (the run was dropped).
    context_since: Instant,
    /// Whether the current foreground app is blacklisted (wallet / password
    /// manager / terminal). Recomputed only when the window changes — opening the
    /// process every keystroke would be wasteful.
    sensitive_app: bool,
    /// The foreground app's executable name (lower case), for its per-app mode.
    /// Recomputed with `sensitive_app`.
    app_exe: Option<String>,
    /// The most recent correction, kept for one-shot Undo (Ctrl+Shift+CapsLock).
    /// Cleared after use and whenever focus/layout changes (an undo that retypes
    /// into a different window/context than the one it corrected would be wrong).
    undo: Option<UndoRecord>,
    /// The layout we requested to switch to, while the target app has not yet
    /// reported it. Keystrokes in that window are translated with *this* layout:
    /// the app processes our posted switch before the next key's input message,
    /// so it already types in the new layout even though `GetKeyboardLayout`
    /// still reports the old one.
    pending_hkl: Option<PendingLayout>,
    /// D-008: set while we own what is on screen for the current run.
    owned: Option<OwnedRun>,
    /// D-009: who has decided what the current token is.
    mark: TokenMark,
    /// The last few completed words, as on screen, for Shift+Backspace right
    /// after a boundary (pressed again: the word before, and so on).
    recent: Recent,
    suggestion: Option<SuggestionRecord>,
    /// A Suggest hint shown while the word is still being typed (Suggest
    /// mode): Tab flips the word in progress. Only where it was shown.
    live_hint: Option<LiveHint>,
}

#[derive(Clone, Copy)]
struct LiveHint {
    hwnd: isize,
    focus_generation: u64,
    created: Instant,
}

impl HookState {
    fn new() -> Self {
        Self {
            buf: WordBuffer::new(),
            seed: secret::SeedTracker::new(),
            last_hwnd: 0,
            last_hkl: 0,
            last_focus_generation: 0,
            context_since: Instant::now(),
            sensitive_app: false,
            app_exe: None,
            undo: None,
            pending_hkl: None,
            owned: None,
            mark: TokenMark::Plain,
            recent: Recent::new(),
            suggestion: None,
            live_hint: None,
        }
    }
}

/// D-009: who has decided what the token in progress is. Reset at every
/// boundary and every time the buffer is dropped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenMark {
    /// Nobody yet: Auto may render and revise it.
    Plain,
    /// RightType converted part of it on its own (an anchored run) and the rest
    /// is arriving natively in the new layout. The buffer holds the whole token
    /// as it is on screen, so its boundary can still revise all of it.
    Converted,
    /// The typist settled it by hand (withdrew our reading, undid an anchor, or
    /// flipped it). Auto leaves it alone; `learn` when what they rejected was
    /// RightType's own conversion, so the finished word is remembered.
    Decided { learn: bool },
}

/// How long a requested layout switch may stay unconfirmed before we assume
/// the app ignored it and fall back to what Windows reports.
const PENDING_LAYOUT_GRACE: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
struct PendingLayout {
    hkl: isize,
    since: Instant,
}

/// D-008: a run whose on-screen text RightType is currently responsible for.
///
/// While this exists every keystroke is swallowed and the screen is moved to the
/// run's current best reading, so a reading chosen early can still be withdrawn.
/// It lives exactly as long as the run in [`WordBuffer`] does and is wiped at the
/// same moments, so revisability costs no extra retention of typed text.
struct OwnedRun {
    /// What we have put on screen for this run.
    rendered: String,
    /// Consecutive keystrokes the Thai reading has survived.
    stable: usize,
}

impl Drop for OwnedRun {
    fn drop(&mut self) {
        self.rendered.zeroize();
    }
}

struct SuggestionRecord {
    /// A word typed with CapsLock on by accident: taking it also turns
    /// CapsLock off.
    caps: bool,
    original: String,
    corrected: String,
    boundary_vk: u16,
    created: Instant,
    /// Where it was made: Tab accepts it only in the same window and field.
    hwnd: isize,
    focus_generation: u64,
}

/// Tab takes a Suggest hint only this soon after it appeared; later, Tab is
/// just Tab again.
const SUGGEST_TAB_WINDOW: Duration = Duration::from_secs(4);

impl Drop for SuggestionRecord {
    fn drop(&mut self) {
        self.original.zeroize();
        self.corrected.zeroize();
    }
}

/// A reversible correction: how many characters to delete, and what to retype
/// to restore the pre-correction text exactly.
struct UndoRecord {
    /// Characters now present in the app (after the correction) to delete.
    injected_len: usize,
    /// Text to retype to restore what was there before.
    restore_text: String,
    kind: UndoKind,
    created_at: Instant,
}

/// Who made a correction, which decides what undoing it teaches us.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UndoKind {
    /// The typist asked for it (hotkey, suggestion): undo just reverts.
    Manual,
    /// RightType corrected a complete word on its own. Undoing it says "that
    /// was a real word", so the restored word is learned.
    AutoWord,
    /// RightType anchored a run mid-word. Undoing it hands the token back to
    /// the typist; it is learned once they finish it.
    AutoMidToken,
    /// RightType put right a word typed with CapsLock on by accident and
    /// turned CapsLock off. Undoing it (Ctrl+Shift+CapsLock, or the next
    /// Shift+Backspace) puts the capitals back and turns CapsLock on again:
    /// they were meant (code, acronyms).
    CapsAccident,
    /// RightType put right a common Thai misspelling (this one). Undoing it
    /// (Shift+Backspace, Ctrl+Shift+CapsLock, or Backspace right after)
    /// puts back what was typed, and that word is not fixed again this run.
    /// The misspelling is kept in `SPELLING_FIXED` until the fix is undone
    /// or another word is fixed.
    Spelling,
    /// A snippet was expanded. Undoing it puts the trigger back; nothing is
    /// learned.
    Snippet,
}

/// The typist's snippets (Settings → Snippets).
static SNIPPETS: std::sync::RwLock<Vec<righttype::snippets::Snippet>> =
    std::sync::RwLock::new(Vec::new());

pub fn snippets() -> Vec<righttype::snippets::Snippet> {
    SNIPPETS.read().map(|l| l.clone()).unwrap_or_default()
}

pub fn set_snippets(list: Vec<righttype::snippets::Snippet>) {
    if let Ok(mut l) = SNIPPETS.write() {
        *l = list;
    }
}

/// Put a snippet's `text` in place of its trigger `word`, then the boundary
/// `vk`. Line breaks are typed as Enter.
/// The local date and time, for a snippet's date and time fields.
pub(crate) fn snippet_now() -> righttype::snippets::Now {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    righttype::snippets::Now {
        year: t.wYear as u32,
        month: t.wMonth as u32,
        day: t.wDay as u32,
        weekday: t.wDayOfWeek as u32,
        hour: t.wHour as u32,
        minute: t.wMinute as u32,
    }
}

unsafe fn expand_snippet(word: &str, vk: u16, text: &str) -> bool {
    // A macro: take the trigger away, then the worker runs its steps (they
    // wait and press keys, so not here on the hook).
    if let Some(Ok(steps)) = righttype::macros::steps(text) {
        let shown = policy::shown_with_caps(word, caps_on());
        inject::expect_before_caret(&shown);
        if !inject::apply(word.chars().count(), "", None) {
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
            return false;
        }
        crate::manual::request_macro(GetForegroundWindow().0 as isize, steps, false);
        return true;
    }
    // Its date and time fields, for now.
    let now = snippet_now();
    let filled = zeroize::Zeroizing::new(righttype::snippets::fill(text, &now));
    let text = filled.as_str();
    let shown = policy::shown_with_caps(word, caps_on());
    inject::expect_before_caret(&shown);
    let lines: Vec<&str> = text.split('\n').collect();
    let chat = is_current_chat_app();
    for (i, line) in lines.iter().enumerate() {
        let delete = if i == 0 { word.chars().count() } else { 0 };
        if i + 1 < lines.len() {
            if chat {
                if !inject::apply(delete, line, None)
                    || !inject::press_chord(&[VK_SHIFT.0, VK_RETURN.0])
                {
                    crate::overlay::show(righttype::i18n::tr(
                        righttype::i18n::T::ErrCorrectionInject,
                    ));
                    return false;
                }
            } else if !inject::apply(delete, line, Some(VK_RETURN.0)) {
                crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
                return false;
            }
        } else if !inject::apply(delete, line, Some(vk)) {
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
            return false;
        }
    }
    let mut restore = format!("{shown}{}", boundary_literal(vk));
    set_undo(text.chars().count() + 1, &restore, UndoKind::Snippet);
    restore.zeroize();
    true
}

/// Put right common Thai misspellings (opt-in; `righttype::spelling`).
static FIX_SPELLING: AtomicBool = AtomicBool::new(false);

pub fn fixes_spelling() -> bool {
    FIX_SPELLING.load(Ordering::Relaxed)
}

pub fn set_fixes_spelling(on: bool) {
    FIX_SPELLING.store(on, Ordering::Relaxed);
}

/// Write English prefixes with their hyphen (`relogin` → `re-login`; on by
/// default; `righttype::english::hyphenated`).
static FIX_HYPHENS: AtomicBool = AtomicBool::new(true);

pub fn fixes_hyphens() -> bool {
    FIX_HYPHENS.load(Ordering::Relaxed)
}

pub fn set_fixes_hyphens(on: bool) {
    FIX_HYPHENS.store(on, Ordering::Relaxed);
}

/// What to do about a key that changes how the next keys type (NumLock off
/// on the keypad, Insert): nothing, say so, or put it right.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyGuard {
    Off,
    Warn,
    /// The keypad: turn NumLock on and type the digit. Insert: hold it back.
    Fix,
}

impl KeyGuard {
    pub fn name(self) -> &'static str {
        match self {
            KeyGuard::Off => "off",
            KeyGuard::Warn => "warn",
            KeyGuard::Fix => "fix",
        }
    }

    pub fn parse(s: &str) -> KeyGuard {
        match s.trim() {
            "off" => KeyGuard::Off,
            "fix" | "block" => KeyGuard::Fix,
            _ => KeyGuard::Warn,
        }
    }

    fn from_u8(v: u8) -> KeyGuard {
        match v {
            0 => KeyGuard::Off,
            2 => KeyGuard::Fix,
            _ => KeyGuard::Warn,
        }
    }
}

static NUMLOCK_MODE: AtomicU8 = AtomicU8::new(1);
static INSERT_MODE: AtomicU8 = AtomicU8::new(1);

pub fn numlock_mode() -> KeyGuard {
    KeyGuard::from_u8(NUMLOCK_MODE.load(Ordering::Relaxed))
}

pub fn set_numlock_mode(mode: KeyGuard) {
    NUMLOCK_MODE.store(mode as u8, Ordering::Relaxed);
}

pub fn insert_mode() -> KeyGuard {
    KeyGuard::from_u8(INSERT_MODE.load(Ordering::Relaxed))
}

pub fn set_insert_mode(mode: KeyGuard) {
    INSERT_MODE.store(mode as u8, Ordering::Relaxed);
}

/// The digit a numeric-keypad key types with NumLock on, for the key it
/// sends with NumLock off (`extended` keys are the separate arrow and
/// editing keys, not the keypad).
fn keypad_digit(vk: u16, extended: bool) -> Option<char> {
    if extended {
        return None;
    }
    Some(match vk {
        0x2D => '0', // Insert
        0x23 => '1', // End
        0x28 => '2', // Down
        0x22 => '3', // Page Down
        0x25 => '4', // Left
        0x0C => '5', // Clear
        0x27 => '6', // Right
        0x24 => '7', // Home
        0x26 => '8', // Up
        0x21 => '9', // Page Up
        _ => return None,
    })
}

/// A warning shown at most once a minute.
static NUM_WARNED: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

fn warn_once(last: &std::sync::Mutex<Option<Instant>>, tag: &str, message: righttype::i18n::T) {
    let mut last = last.lock().unwrap();
    if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
        return;
    }
    *last = Some(Instant::now());
    trace_note("keypad with NumLock off: said so");
    crate::overlay::badge_at_caret(tag);
    crate::overlay::show(righttype::i18n::tr(message));
}

/// Offer the rest of a long Thai word (Tab takes it): opt-in.
static COMPLETE_THAI: AtomicBool = AtomicBool::new(false);

pub fn completes_thai() -> bool {
    COMPLETE_THAI.load(Ordering::Relaxed)
}

pub fn set_completes_thai(on: bool) {
    COMPLETE_THAI.store(on, Ordering::Relaxed);
}

/// How long a completion on offer can be taken.
const COMPLETION_OPEN: Duration = Duration::from_secs(5);

struct Completion {
    /// What Tab types: the letters after those typed.
    rest: String,
    hwnd: isize,
    focus_generation: u64,
    created: Instant,
}

impl Drop for Completion {
    fn drop(&mut self) {
        self.rest.zeroize();
    }
}

thread_local! {
    static COMPLETION: RefCell<Option<Completion>> = const { RefCell::new(None) };
}

/// The Thai word being typed (at least three letters) has one sure way on
/// (`righttype::dict::Dictionary::sure_completion`): show it, for Tab.
fn offer_completion() {
    let run = STATE.with(|s| s.borrow().buf.current().to_string());
    let typed = run.chars().count();
    let thai = run.chars().all(|c| ('\u{0E01}'..='\u{0E4E}').contains(&c));
    if typed < 3 || !thai || STATE.with(|s| s.borrow().seed.guarding()) {
        let mut run = run;
        run.zeroize();
        return;
    }
    if let Some(mut whole) = dict::thai().sure_completion(&run, 2) {
        let rest: String = whole.chars().skip(typed).collect();
        let mut hint = format!("→ {whole}  ·  Tab");
        crate::overlay::show_at(&hint, crate::caret::hint_anchor());
        hint.zeroize();
        whole.zeroize();
        COMPLETION.with(|c| {
            *c.borrow_mut() = Some(Completion {
                rest,
                hwnd: unsafe { GetForegroundWindow() }.0 as isize,
                focus_generation: crate::focus::generation(),
                created: Instant::now(),
            })
        });
    }
    let mut run = run;
    run.zeroize();
}

/// Ghost suggestions (2.4 C): `->` offers →, `x^2` offers x², for Tab.
static GHOSTS: AtomicBool = AtomicBool::new(true);

pub fn ghosts() -> bool {
    GHOSTS.load(Ordering::Relaxed)
}

pub fn set_ghosts(on: bool) {
    GHOSTS.store(on, Ordering::Relaxed);
    if !on {
        ghost_forget();
    }
}

struct GhostOffer {
    replace: usize,
    text: String,
    hwnd: isize,
    focus_generation: u64,
    created: Instant,
}

thread_local! {
    /// The text before the caret, for ghost suggestions: in memory only,
    /// zeroized when let go (`righttype::ghost::Tail`).
    static GHOST_TYPED: RefCell<righttype::ghost::Tail> =
        const { RefCell::new(righttype::ghost::Tail::new()) };
    static GHOST: RefCell<Option<GhostOffer>> = const { RefCell::new(None) };
}

/// Forget the text kept before the caret (never panics if it is in use).
fn ghost_tail_forget() {
    GHOST_TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.forget();
        }
    });
}

fn ghost_forget() {
    ghost_tail_forget();
    if GHOST.with(|g| g.borrow_mut().take()).is_some() {
        crate::overlay::dismiss();
    }
}

/// Follows what a key did to the text before the caret: a character or a
/// space adds to it, Backspace takes one off, anything else (Enter, Tab,
/// arrows, a chord) forgets it. Taken back in [`ghost_settle`] if the key
/// is kept from the app.
fn ghost_track(key: Key, vk: u16) {
    GHOST_TYPED.with(|t| {
        let mut t = t.borrow_mut();
        match key {
            Key::Char(c) => t.typed(c),
            Key::Backspace => t.backspace(),
            Key::Boundary if vk == VK_SPACE.0 => t.typed(' '),
            _ => t.forget(),
        }
    });
}

/// RightType wrote `text` over the `backspaces` characters before the caret
/// (a word put into the other layout, a snippet, Undo): the text kept for
/// ghost suggestions follows the screen, so `x de]y' 10` corrected to
/// `x กำลัง 10` is still read as math.
pub(crate) fn ghost_applied(backspaces: usize, text: &str, trailing_vk: Option<u16>) {
    use righttype::ghost::Then;
    let then = match trailing_vk {
        None => Then::Nothing,
        Some(vk) if vk == VK_SPACE.0 => Then::Space,
        Some(_) => Then::Other,
    };
    GHOST_TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.written(backspaces, text, then);
        }
    });
}

/// A key starts (not nested): nothing of it is pending yet.
fn ghost_begin_key() {
    GHOST_TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.begin_key();
        }
    });
}

/// The key has been handled: if it was kept from the app, take back what
/// it did to the text kept for ghost suggestions.
fn ghost_settle(swallowed: bool) {
    GHOST_TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.settle(swallowed);
        }
    });
}

/// RightType wrote over the word in progress itself (a ghost taken, a
/// character stepped with Right Alt): nothing about that word is the hook's
/// to decide or redraw any more. A run it owned stays on screen as it is
/// now — reconciling it would backspace over what was just written.
fn word_overwritten() {
    STATE.with(|s| {
        if let Ok(mut st) = s.try_borrow_mut() {
            st.buf.clear();
            st.owned = None;
            st.mark = TokenMark::Plain;
            st.recent.clear();
            st.suggestion = None;
            st.live_hint = None;
        }
    });
}

/// The hint for an offer: what was typed, what Tab makes of it.
pub fn ghost_hint(typed: &str, symbol: &str) -> String {
    format!("{typed}  →  {symbol}   ·   Tab")
}

/// Whether a character just typed may bring up a ghost suggestion here.
fn ghost_here(key: Key, mode_now: Mode) -> bool {
    matches!(key, Key::Char(_))
        && ghosts()
        && mode_now != Mode::Code
        && !crate::focus::completes_inline()
        && !current_app().is_some_and(|e| righttype::code::is_code_editor(&e))
}

/// After a character: show the symbol it can become, for Tab.
fn offer_ghost() {
    let offer = GHOST_TYPED.with(|t| righttype::ghost::offer(t.borrow().as_str()));
    let Some(offer) = offer else {
        return;
    };
    let mut typed: String = GHOST_TYPED.with(|t| {
        let t = t.borrow();
        let t = t.as_str();
        let skip = t.chars().count().saturating_sub(offer.replace);
        t.chars().skip(skip).collect()
    });
    let mut hint = ghost_hint(&typed, &offer.text);
    typed.zeroize();
    crate::overlay::offer_at(&hint, crate::caret::hint_anchor());
    hint.zeroize();
    e2e_trace(format!(
        "ghost offered: {}",
        offer
            .text
            .chars()
            .map(|c| format!("U+{:04X}", c as u32))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    GHOST.with(|g| {
        *g.borrow_mut() = Some(GhostOffer {
            replace: offer.replace,
            text: offer.text,
            hwnd: unsafe { GetForegroundWindow() }.0 as isize,
            focus_generation: crate::focus::generation(),
            created: Instant::now(),
        })
    });
}

/// Right Alt tapped after a character steps it through its set (2.4).
static CYCLES: AtomicBool = AtomicBool::new(true);

pub fn cycles_characters() -> bool {
    CYCLES.load(Ordering::Relaxed)
}

pub fn set_cycles_characters(on: bool) {
    CYCLES.store(on, Ordering::Relaxed);
}

static CHAR_SETS: std::sync::RwLock<Option<righttype::charsets::Sets>> =
    std::sync::RwLock::new(None);

/// Every key's set of characters (the defaults with the typist's own).
pub fn char_sets() -> righttype::charsets::Sets {
    CHAR_SETS
        .read()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_default()
}

pub fn set_char_sets(sets: righttype::charsets::Sets) {
    if let Ok(mut s) = CHAR_SETS.write() {
        *s = Some(sets);
    }
}

/// Pin `ch` to `key`, or take it off; saved. True when it is pinned.
pub fn pin_char(key: char, ch: &str) -> bool {
    let mut sets = char_sets();
    let on = sets.toggle(key, ch);
    set_char_sets(sets);
    crate::config::persist();
    on
}

/// A character being stepped through its set with Right Alt.
struct Cycle {
    typed: String,
    set: Vec<String>,
    /// 0: as typed; n: the set's n-th.
    at: usize,
    shown: String,
    hwnd: isize,
    focus_generation: u64,
    when: Instant,
}

impl Drop for Cycle {
    fn drop(&mut self) {
        self.typed.zeroize();
        self.shown.zeroize();
    }
}

thread_local! {
    /// When Right Alt went down, while nothing else has been pressed since.
    static RALT_DOWN: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static CYCLE: RefCell<Option<Cycle>> = const { RefCell::new(None) };
}

/// The longest press of Right Alt that is a tap.
const RALT_TAP: Duration = Duration::from_millis(350);

/// Right Alt was tapped: step the character before the caret (the last one
/// typed) to the next in its set, or start with it. False when there is
/// nothing to step (the tap is then Windows' own).
unsafe fn cycle_character() -> bool {
    if !cycles_characters()
        || STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        return false;
    }
    let hwnd = GetForegroundWindow().0 as isize;
    let generation = crate::focus::generation();
    let fresh = CYCLE.with(|c| {
        c.borrow().as_ref().is_some_and(|c| {
            c.hwnd == hwnd
                && c.focus_generation == generation
                && c.when.elapsed() < Duration::from_millis(crate::overlay::OFFER_MS.into())
        })
    });
    if !fresh {
        let typed = GHOST_TYPED.with(|t| t.borrow().last());
        let Some(typed) = typed.filter(|c| !c.is_whitespace()) else {
            return false;
        };
        let set = char_sets().of(typed);
        if set.is_empty() {
            return false;
        }
        CYCLE.with(|c| {
            *c.borrow_mut() = Some(Cycle {
                typed: typed.to_string(),
                set,
                at: 0,
                shown: typed.to_string(),
                hwnd,
                focus_generation: generation,
                when: Instant::now(),
            })
        });
    }
    let step = CYCLE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        c.at = (c.at + 1) % (c.set.len() + 1);
        let next = if c.at == 0 {
            c.typed.clone()
        } else {
            c.set[c.at - 1].clone()
        };
        let old = std::mem::replace(&mut c.shown, next.clone());
        c.when = Instant::now();
        Some((
            old,
            next,
            righttype::charsets::strip(&c.typed, &c.set, c.at),
        ))
    });
    let Some((mut old, next, mut strip)) = step else {
        return false;
    };
    let n = old.chars().count();
    old.zeroize();
    if !inject::apply(n, &next, None) {
        CYCLE.with(|c| c.borrow_mut().take());
        return false;
    }
    word_overwritten();
    // What is before the caret now followed what was written
    // (`ghost_applied`), for the next tap and ghost offers.
    crate::overlay::offer_at(&strip, crate::caret::hint_anchor());
    strip.zeroize();
    trace_note("Right Alt: character stepped");
    true
}

/// Esc while stepping: the character as typed again. True when it was.
unsafe fn cycle_back() -> bool {
    let Some(c) = CYCLE.with(|c| c.borrow_mut().take()) else {
        return false;
    };
    if c.at == 0
        || c.hwnd != GetForegroundWindow().0 as isize
        || c.focus_generation != crate::focus::generation()
        || c.when.elapsed() >= Duration::from_millis(crate::overlay::OFFER_MS.into())
    {
        return false;
    }
    crate::overlay::dismiss();
    let done = inject::apply(c.shown.chars().count(), &c.typed, None);
    word_overwritten();
    done
}

/// Why the last word was fixed or left (a `righttype::why::Why`).
static LAST_WHY: AtomicU8 = AtomicU8::new(0);

pub fn last_why() -> righttype::why::Why {
    righttype::why::Why::from_u8(LAST_WHY.load(Ordering::Relaxed))
}

/// The grave key types its character instead of switching the language.
static GRAVE_TYPES: AtomicBool = AtomicBool::new(false);

pub fn grave_types() -> bool {
    GRAVE_TYPES.load(Ordering::Relaxed)
}

pub fn set_grave_types(on: bool) {
    GRAVE_TYPES.store(on, Ordering::Relaxed);
}

/// A language switch that comes with a shortcut is undone.
static GUARD_SWITCH: AtomicBool = AtomicBool::new(false);

pub fn guards_switch() -> bool {
    GUARD_SWITCH.load(Ordering::Relaxed)
}

pub fn set_guards_switch(on: bool) {
    GUARD_SWITCH.store(on, Ordering::Relaxed);
}

/// A switch this soon after a Ctrl/Alt + Shift shortcut came with it.
const SHORTCUT_SWITCH_WINDOW: Duration = Duration::from_millis(700);

thread_local! {
    /// The last Ctrl/Alt + Shift + key shortcut: when, and the keyboard
    /// layout it was pressed on.
    static SHORTCUT: std::cell::Cell<Option<(Instant, isize)>> =
        const { std::cell::Cell::new(None) };
}

/// Ctrl+Backspace after Thai deletes one Thai word (Windows takes the whole
/// run of Thai, which has no spaces between words). On by default.
static DELETE_THAI_WORDS: AtomicBool = AtomicBool::new(true);

pub fn deletes_thai_words() -> bool {
    DELETE_THAI_WORDS.load(Ordering::Relaxed)
}

pub fn set_deletes_thai_words(on: bool) {
    DELETE_THAI_WORDS.store(on, Ordering::Relaxed);
}

/// In a chat app, Enter on a message that looks typed on the wrong keyboard
/// is held once (see `guard_enter`). On by default.
static GUARD_ENTER: AtomicBool = AtomicBool::new(true);

pub fn guards_enter() -> bool {
    GUARD_ENTER.load(Ordering::Relaxed)
}

pub fn set_guards_enter(on: bool) {
    GUARD_ENTER.store(on, Ordering::Relaxed);
}

/// Chat apps added in the config (see `righttype::per_app::is_chat_app`).
static CHAT_APPS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

pub fn chat_apps() -> Vec<String> {
    CHAT_APPS.read().map(|a| a.clone()).unwrap_or_default()
}

pub fn set_chat_apps(apps: Vec<String>) {
    if let Ok(mut a) = CHAT_APPS.write() {
        *a = apps;
    }
}

/// How long a held Enter waits for the second press that sends anyway.
const ENTER_HELD_FOR: Duration = Duration::from_secs(5);

thread_local! {
    /// Enter was held here (this window) at this moment: the next Enter
    /// soon after sends.
    static ENTER_HELD: std::cell::Cell<Option<(isize, Instant)>> =
        const { std::cell::Cell::new(None) };
}

/// The focused program marks text with the keyboard language at arrival
/// (Word): see `righttype::compat::tags_language`.
pub(crate) fn tags_language_here() -> bool {
    current_app().is_some_and(|e| righttype::compat::tags_language(&e))
}

/// Before text is written into a program that tags it by keyboard language,
/// put the keyboard of that text on (Thai text, Thai keyboard), so the text
/// carries the right proofing language. Text with no letters is left alone.
pub(crate) unsafe fn prepare_layout_for(text: &str) {
    if !tags_language_here() {
        return;
    }
    let letters = text
        .chars()
        .any(|c| c.is_ascii_alphabetic() || ('\u{0E01}'..='\u{0E4E}').contains(&c));
    if !letters {
        return;
    }
    let target = layout_of(text);
    if policy::supported_layout_id(layout_id(effective_layout())) != Some(target) {
        activate_layout(target);
        // Give the target app (e.g. Word) a moment to process the posted
        // WM_INPUTLANGCHANGEREQUEST so proofing tags match the incoming text.
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn is_current_chat_app() -> bool {
    STATE
        .with(|s| s.borrow().app_exe.clone())
        .is_some_and(|exe| {
            CHAT_APPS
                .read()
                .is_ok_and(|extra| righttype::per_app::is_chat_app(&exe, &extra))
        })
}

/// Hold this Enter? In a chat app (with the guard on, not in Code mode),
/// when the words of the message so far look typed on the wrong keyboard
/// and this is not the second Enter that sends anyway.
unsafe fn hold_enter(mode_now: Mode) -> bool {
    let hwnd = GetForegroundWindow().0 as isize;
    let held = ENTER_HELD.with(|h| h.take());
    if held.is_some_and(|(w, at)| w == hwnd && at.elapsed() < ENTER_HELD_FOR) {
        return false;
    }
    if !guards_enter() || mode_now == Mode::Code {
        return false;
    }
    if !is_current_chat_app() {
        return false;
    }
    let mut words = STATE.with(|s| {
        let st = s.borrow();
        let mut w = st.recent.words();
        let current = st.buf.current();
        if !current.is_empty() {
            w.push(current.to_string());
        }
        w
    });
    let refs: Vec<&str> = words.iter().map(|w| w.as_str()).collect();
    let hold = righttype::repair::looks_mistyped(
        &refs,
        righttype::dict::english(),
        righttype::dict::thai(),
    );
    drop(refs);
    words.iter_mut().for_each(|w| w.zeroize());
    if hold {
        ENTER_HELD.with(|h| h.set(Some((hwnd, Instant::now()))));
        trace_note("enter held: the message looks typed on the wrong keyboard");
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastEnterHeld));
    }
    hold
}

/// A Backspace this soon after a spelling fix takes the fix back instead of
/// deleting: someone surprised by a changed word reaches for Backspace, and
/// deleting into a fix they did not expect leaves a mess.
const SPELLING_GRACE: Duration = Duration::from_millis(1500);

thread_local! {
    /// When the last spelling fix was made (for [`SPELLING_GRACE`]).
    static SPELLING_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    /// Misspellings the typist took a fix back for: left alone from now on.
    static SPELLING_KEPT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The misspelling the last spelling fix put right.
    static SPELLING_FIXED: RefCell<String> = const { RefCell::new(String::new()) };
}

impl Drop for UndoRecord {
    fn drop(&mut self) {
        self.restore_text.zeroize();
    }
}

/// Record `restore_text` (what to retype) as the one-shot Undo target for the
/// correction that just replaced it with `injected_len` characters. Refuses
/// secret-shaped text — `detect::detect` already guards the automatic paths, but
/// the manual convert-word hotkey doesn't, so this is the one place that matters.
fn set_undo(injected_len: usize, restore_text: &str, kind: UndoKind) {
    let record = (!secret::is_secret_token(restore_text)).then(|| UndoRecord {
        injected_len,
        restore_text: restore_text.to_string(),
        kind,
        created_at: Instant::now(),
    });
    STATE.with(|s| s.borrow_mut().undo = record);
}

/// A fix RightType made here was taken back. Enough of them in one app
/// offer a calmer mode there (see `apps::note_rejection`).
fn note_rejection() {
    use righttype::i18n::{tr, trf, T};
    let Some(exe) = STATE.with(|s| s.borrow().app_exe.clone()) else {
        return;
    };
    let Some(mode) = mode_here() else {
        return;
    };
    if let Some(calmer) = crate::apps::note_rejection(&exe, mode.into()) {
        diag::note(
            "fixes taken back three times in one app: calmer mode offered",
            &[],
        );
        let keys = hotkeys().chord(Action::Palette).format();
        crate::overlay::show(&trf(
            T::ToastOfferMode,
            &[
                ("app", &exe),
                ("mode", tr(crate::tray::app_mode_name(calmer))),
                ("keys", &keys),
            ],
        ));
    }
}

/// Ctrl+Shift+CapsLock: revert the most recent correction, if any. One-shot —
/// the record is consumed whether or not this call finds one.
unsafe fn undo_last_correction() {
    let Some(rec) = STATE.with(|s| s.borrow_mut().undo.take()) else {
        e2e_trace("undo: no record".to_string());
        diag::note("undo: nothing to undo", &[]);
        return;
    };
    if rec.created_at.elapsed() > Duration::from_secs(30) {
        e2e_trace("undo: record expired".to_string());
        diag::note("undo: older than 30 s", &[]);
        return;
    }
    let ok = inject::apply(rec.injected_len, &rec.restore_text, None);
    e2e_trace(format!("undo apply len={} -> {ok}", rec.injected_len));
    diag::note(
        "undo",
        &[("deleted", rec.injected_len.into()), ("ok", ok.into())],
    );
    if ok {
        let restored = rec.restore_text.trim_end_matches(['\r', '\t', ' ']);
        // The word counted was the correction; the one kept is the original.
        habit_correction(!has_thai(restored), has_thai(restored));
        if !matches!(rec.kind, UndoKind::Manual | UndoKind::Snippet) {
            note_rejection();
            if let Some(exe) = current_app() {
                crate::stats::app_event(&exe, righttype::app_quality::Event::Undone);
            }
        }
        let mut kept_spelling = None;
        match rec.kind {
            UndoKind::Manual | UndoKind::Snippet => {}
            UndoKind::Spelling => {
                let wrong = SPELLING_FIXED.with(|f| std::mem::take(&mut *f.borrow_mut()));
                SPELLING_KEPT.with(|k| k.borrow_mut().push(wrong.clone()));
                SPELLING_AT.with(|t| t.set(None));
                kept_spelling = Some(wrong);
            }
            UndoKind::CapsAccident => {
                if !caps_on() {
                    inject::toggle_capslock();
                }
                crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastCapsKept));
            }
            UndoKind::AutoWord => crate::learn::learn_now(restored),
            UndoKind::AutoMidToken => STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.buf.replace(restored);
                st.mark = TokenMark::Decided { learn: true };
            }),
        }
        // The typist meant what they typed: keep typing it in its own layout.
        activate_layout(layout_of(restored));
        match kept_spelling {
            Some(wrong) => crate::overlay::show_at(
                &righttype::i18n::trf(righttype::i18n::T::ToastSpellingKept, &[("word", &wrong)]),
                crate::caret::hint_anchor(),
            ),
            None => crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo)),
        }
    } else {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrUndoInject));
    }
}

/// The literal character a boundary key inserts (matches what a real keypress
/// would produce as `WM_CHAR`), for reconstructing exact Undo text.
fn boundary_literal(vk: u16) -> char {
    if vk == VK_RETURN.0 {
        '\r'
    } else if vk == VK_TAB.0 {
        '\t'
    } else {
        ' '
    }
}

/// The boundary key that types `c` (the inverse of [`boundary_literal`]).
fn boundary_vk(c: char) -> u16 {
    match c {
        '\r' => VK_RETURN.0,
        '\t' => VK_TAB.0,
        _ => VK_SPACE.0,
    }
}

/// The installed hook handle, kept only so [`uninstall`] can remove it. The raw
/// handle is not `Send`; this wrapper asserts it is safe to move between threads
/// (we only ever touch it from install/uninstall, never concurrently).
struct HookHandle(HHOOK);
unsafe impl Send for HookHandle {}
static HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);
/// The low-level mouse hook: a click can move the caret without a keystroke.
static MOUSE_HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);

/// `GetTickCount` time of the last event the hook received (or of its
/// installation). The session watchdog compares it with the system's last-input
/// time to notice a hook Windows removed without any power/session event.
static LAST_HOOK_TICK: AtomicU32 = AtomicU32::new(0);

/// See [`LAST_HOOK_TICK`].
pub fn last_hook_tick() -> u32 {
    LAST_HOOK_TICK.load(Ordering::Relaxed)
}

/// Is `vk` physically held right now? Read from the real async key state so it
/// can never go stale (the reason we don't track modifiers from the event stream).
unsafe fn is_down(vk: VIRTUAL_KEY) -> bool {
    (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0
}

/// Is CapsLock currently toggled on?
unsafe fn caps_on() -> bool {
    (GetKeyState(VK_CAPITAL.0 as i32) & 0x0001) != 0
}

/// The modifier virtual-keys currently held down, for the injector to release
/// before a correction (the Bug 2 fix).
pub fn held_modifiers() -> Vec<u16> {
    let mut v = Vec::new();
    unsafe {
        if is_down(VK_SHIFT) {
            v.push(VK_SHIFT.0);
        }
        if is_down(VK_CONTROL) {
            v.push(VK_CONTROL.0);
        }
        if is_down(VK_MENU) {
            v.push(VK_MENU.0);
        }
    }
    v
}

/// Install the low-level keyboard hook for this thread.
///
/// # Safety
/// The calling thread must run a message loop for the duration of the hook, and
/// must call [`uninstall`] before exiting.
pub unsafe fn install() -> windows::core::Result<()> {
    let hmod = GetModuleHandleW(None)?;
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), HINSTANCE(hmod.0), 0)?;
    *HOOK.lock().unwrap() = Some(HookHandle(hook));
    detect_keyboards();
    // Best-effort: without it a click is only noticed when focus changes.
    if let Ok(mouse) = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE(hmod.0), 0) {
        *MOUSE_HOOK.lock().unwrap() = Some(HookHandle(mouse));
    }
    LAST_HOOK_TICK.store(
        windows::Win32::System::SystemInformation::GetTickCount(),
        Ordering::Relaxed,
    );
    // RAM hardening: pin the word buffer's (already-stable) allocation in
    // physical RAM so a typed secret can never be paged to disk. Locking it
    // here, once, is safe precisely because `WordBuffer` pre-reserves its
    // capacity and never reallocates for its lifetime (see `stable_region`).
    STATE.with(|s| {
        let (ptr, len) = s.borrow().buf.stable_region();
        let _ = crate::ram::lock_region(ptr, len);
    });
    Ok(())
}

/// Remove the hook if installed.
///
/// # Safety
/// Must be called on the same thread that called [`install`].
pub unsafe fn uninstall() {
    if let Some(h) = HOOK.lock().unwrap().take() {
        let _ = UnhookWindowsHookEx(h.0);
    }
    if let Some(h) = MOUSE_HOOK.lock().unwrap().take() {
        let _ = UnhookWindowsHookEx(h.0);
    }
}

/// Tear down and re-establish the hook — the recovery action after a power/session
/// transition that may have silently evicted it (RightLang Bug 1).
///
/// # Safety
/// Same thread + message-loop requirements as [`install`].
pub unsafe fn reinstall() -> windows::core::Result<()> {
    uninstall();
    install()
}

/// A mouse button went down: the click may have moved the caret, so nothing
/// recorded about the text before it can be trusted any more — the same as an
/// arrow key. Only the event type is looked at, never where the click was.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // The keyboard lock with the mouse locked too (cleaning).
    if code == HC_ACTION as i32 && crate::lock::mouse_event() {
        return LRESULT(1);
    }
    // A click or the wheel while Ctrl is held: not a hold for the list.
    if code == HC_ACTION as i32 && wparam.0 as u32 != 0x0200
    /* WM_MOUSEMOVE */
    {
        ctrl_hold_input(None, false, false);
        shift_tap_input(None, false);
    }
    if code == HC_ACTION as i32
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
    {
        caret_may_have_moved();
        // A click anywhere else puts away the panes that do not take the
        // focus: the list at the text cursor, the app's shortcuts, the list
        // for a held key.
        let at = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        crate::caretlist::click_at(at.pt.x, at.pt.y);
        crate::sheet::click_at(at.pt.x, at.pt.y);
        if PICK
            .with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.take()))
            .is_some()
        {
            crate::overlay::dismiss();
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Let go of everything that assumes the caret is where typing left it: the
/// word in progress, a run we own (left on screen as it is), the Undo record,
/// the recent words and a pending suggestion.
fn caret_may_have_moved() {
    ghost_tail_forget();
    let offered = GHOST.with(|g| g.try_borrow_mut().ok().and_then(|mut g| g.take()));
    if offered.is_some() {
        crate::overlay::dismiss();
    }
    STATE.with(|s| {
        // Never re-entered from inside the keyboard path, but do not panic if
        // a nested hook call ever finds the state borrowed.
        let Ok(mut st) = s.try_borrow_mut() else {
            return;
        };
        st.buf.clear();
        st.owned = None;
        st.mark = TokenMark::Plain;
        st.undo = None;
        st.recent.clear();
        st.suggestion = None;
        st.live_hint = None;
    });
}

unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        LAST_HOOK_TICK.store(kb.time, Ordering::Relaxed);
        // The keyboard lock (cleaning, the key tester) comes before
        // everything: no key gets through, ours aside.
        let msg = wparam.0 as u32;
        if kb.dwExtraInfo != INJECT_TAG
            && crate::lock::key_event(
                kb.scanCode as u16,
                kb.flags.0 & 0x01 != 0,
                msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN,
            )
        {
            return LRESULT(1);
        }
        // Typing practice in front: its keys go to it, by their place on
        // the keyboard (so a keyboard not installed can be practised), and
        // nowhere else. Alt and Windows-key shortcuts still work.
        if kb.dwExtraInfo != INJECT_TAG && practice_takes(kb, msg) {
            return LRESULT(1);
        }
        // A key that types twice by itself: counted, and dropped for the
        // keys the typist chose to filter. Hardware only (and the e2e
        // test's keys): other programs' keys do not bounce.
        if kb.dwExtraInfo != INJECT_TAG
            && ((kb.flags.0 & LLKHF_INJECTED.0) == 0 || debug_e2e_accepts_injected())
        {
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            if down && !is_modifier(kb.vkCode as u16) {
                FAST_RUN.with(|f| {
                    let (last, run) = f.get();
                    let quick = kb.time.wrapping_sub(last) <= SCANNER_GAP_MS;
                    f.set((kb.time, if quick { run + 1 } else { 0 }));
                });
            }
            if fake_keyboard_blocks(kb.vkCode as u16, down, kb.time) {
                return LRESULT(1);
            }
            let key = (kb.scanCode as u16, kb.flags.0 & 0x01 != 0);
            if down && !is_modifier(kb.vkCode as u16) {
                comfort_key(kb.vkCode as u16);
            }
            match CHATTER.with(|c| c.borrow_mut().observe(key, down, kb.time)) {
                righttype::chatter::Verdict::Pass => {}
                righttype::chatter::Verdict::Bounce => {
                    e2e_trace(format!("key bounce: scan {:#x}", key.0));
                    diag::note("key bounce", &[]);
                }
                righttype::chatter::Verdict::Drop => {
                    e2e_trace(format!("key bounce dropped: scan {:#x}", key.0));
                    return LRESULT(1);
                }
            }
        }
        // Skip anything we generated: our tag is authoritative and timing-free.
        let externally_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        let ours = kb.dwExtraInfo == INJECT_TAG
            || (externally_injected && !debug_e2e_accepts_injected())
            || INJECTING.load(Ordering::Relaxed);
        if ours && wparam.0 as u32 == WM_KEYDOWN {
            e2e_trace(format!(
                "key vk={:#x} skipped (tag={} injecting={})",
                kb.vkCode,
                kb.dwExtraInfo == INJECT_TAG,
                INJECTING.load(Ordering::Relaxed)
            ));
        }
        if !ours && wparam.0 as u32 == WM_KEYDOWN {
            crate::verify::TYPED.fetch_add(1, Ordering::SeqCst);
        }
        if !ours {
            // A key can arrive while the previous one is still being handled:
            // waiting on a slow text box (SendMessageTimeout) lets Windows
            // call this hook again on the same thread. Handling it then
            // corrected a word twice (CI: 'สสวัสดี'). Such a key goes through
            // untouched, and the word in progress is dropped once the outer
            // call is done: what is on screen is no longer known. (Keys that
            // arrive while our own keys are being sent are handled as before:
            // passing those through turned a Shift+Backspace into a plain
            // Backspace — CI, Edge.)
            let nested = PROCESSING.with(|p| p.get());
            // The key being handled (or one already passed through) handed
            // to the hook again: Windows does that when the hook is slow to
            // return. It is the same key press (same time stamp), and was
            // passed on or is being decided already (CI: `เรียนo`, the `o`
            // that finished `giupo` typed after the fix).
            let event = (kb.vkCode, kb.time, wparam.0 as u32);
            if nested && SEEN.with(|s| s.borrow().contains(&event)) {
                e2e_trace(format!(
                    "key vk={:#x} handed to the hook again: dropped",
                    kb.vkCode
                ));
                return LRESULT(1);
            }
            if nested && crate::focus::waiting_on_app() {
                SEEN.with(|s| s.borrow_mut().push(event));
                NESTED_KEY.with(|n| n.set(true));
                e2e_trace(format!(
                    "key vk={:#x} arrived while busy: passed through",
                    kb.vkCode
                ));
                return CallNextHookEx(HHOOK::default(), code, wparam, lparam);
            }
            // Restored however `process` ends.
            struct Done(bool);
            impl Drop for Done {
                fn drop(&mut self) {
                    PROCESSING.with(|p| p.set(self.0));
                }
            }
            PROCESSING.with(|p| p.set(true));
            if !nested {
                SEEN.with(|s| {
                    let mut s = s.borrow_mut();
                    s.clear();
                    s.push(event);
                });
            }
            let done = Done(nested);
            let started = Instant::now();
            if !nested {
                HOOK_DEADLINE.with(|d| d.set(Some(started + HOOK_BUDGET)));
            }
            if !nested {
                ghost_begin_key();
            }
            let swallow = process(wparam.0 as u32, kb);
            if !nested {
                ghost_settle(swallow);
            }
            if !nested {
                HOOK_DEADLINE.with(|d| d.set(None));
            }
            drop(done);
            if !nested {
                righttype::timing::HOOK.record(started.elapsed().as_micros() as u64);
                // A slow key is where keys typed meanwhile can slip past.
                if started.elapsed() > Duration::from_millis(100) {
                    e2e_trace(format!(
                        "slow key vk={:#x}: {} ms in the hook",
                        kb.vkCode,
                        started.elapsed().as_millis()
                    ));
                }
                let waited = crate::focus::take_wait_us();
                if waited > 0 {
                    righttype::timing::WAITING.record(waited);
                }
            }
            if NESTED_KEY.with(|n| n.replace(false)) {
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.buf.clear();
                    st.owned = None;
                    st.mark = TokenMark::Plain;
                    st.recent.clear();
                });
            }
            if swallow {
                // We handled this key as a hotkey/correction; swallow it.
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// A key pressed on a keyboard: the rest reminder counts it, and keys on
/// screen show it if it is a shortcut — never in a password field or an app
/// on the safety list.
unsafe fn comfort_key(vk: u16) {
    crate::rest::key();
    if !crate::onscreen::enabled()
        || STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        return;
    }
    let mods = righttype::keycast::Mods {
        ctrl: is_down(VK_CONTROL),
        alt: is_down(VK_MENU),
        shift: is_down(VK_SHIFT),
        win: is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C)),
    };
    if let Some(label) = righttype::keycast::label(vk, mods) {
        crate::onscreen::show(label);
    }
}

thread_local! {
    /// Keys that type twice by themselves (see righttype::chatter).
    static CHATTER: RefCell<righttype::chatter::Chatter> =
        RefCell::new(righttype::chatter::Chatter::new());
}

pub fn set_debounce_keys(keys: Vec<righttype::chatter::KeyId>) {
    CHATTER.with(|c| c.borrow_mut().set_filtered(keys));
}

pub fn debounce_keys() -> Vec<righttype::chatter::KeyId> {
    CHATTER.with(|c| c.borrow().filtered().to_vec())
}

/// Keys seen bouncing (scan code and extended flag) and how often.
pub fn chatter_suspects() -> Vec<(righttype::chatter::KeyId, u32)> {
    CHATTER.with(|c| c.borrow().suspects())
}

/// Keys this close together (ms) come from a machine, not fingers: a
/// barcode scanner (or a device pretending to be a keyboard). Windows
/// stamps keys with a clock that ticks every 15.6 ms, so keys sent back to
/// back can read 16 ms apart.
const SCANNER_GAP_MS: u32 = 20;
/// A scanner's burst is at least this many characters.
const SCANNER_MIN: usize = 6;

thread_local! {
    /// The last key's time, and how many keys in a row came within
    /// [`SCANNER_GAP_MS`] of the one before.
    static FAST_RUN: std::cell::Cell<(u32, usize)> = const { std::cell::Cell::new((0, 0)) };
}

static FIXES_SCANNERS: AtomicBool = AtomicBool::new(true);
static GUARDS_FAKE_KEYBOARDS: AtomicBool = AtomicBool::new(true);

pub fn fixes_scanners() -> bool {
    FIXES_SCANNERS.load(Ordering::Relaxed)
}
pub fn set_fixes_scanners(on: bool) {
    FIXES_SCANNERS.store(on, Ordering::Relaxed);
}
pub fn guards_fake_keyboards() -> bool {
    GUARDS_FAKE_KEYBOARDS.load(Ordering::Relaxed)
}
pub fn set_guards_fake_keyboards(on: bool) {
    GUARDS_FAKE_KEYBOARDS.store(on, Ordering::Relaxed);
}

/// After Win+R, this long (ms) for a fast run to count as a device typing a
/// command into the Run box.
const RUN_BOX_WINDOW_MS: u32 = 3000;
/// A run this long, as fast as a machine, is a device typing.
const FAKE_RUN: usize = 8;
/// Keys stay held back until the device has been quiet this long (ms).
const FAKE_QUIET_MS: u32 = 1000;

thread_local! {
    /// When Win+R was pressed; whether keys are being held back now.
    static RUN_BOX_AT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    static HOLDING_FAKE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// A device posing as a keyboard (a "BadUSB" stick) opens the Run box with
/// Win+R and types a command into it faster than any hand, then Enter. Once
/// such a run starts, every key is held back (Enter included) until the
/// device goes quiet; the typist is told. A hand never types 8 keys within
/// 20 ms of each other.
unsafe fn fake_keyboard_blocks(vk: u16, down: bool, time: u32) -> bool {
    if !guards_fake_keyboards() {
        return false;
    }
    if let Some(last) = HOLDING_FAKE.with(|h| h.get()) {
        if time.wrapping_sub(last) < FAKE_QUIET_MS {
            HOLDING_FAKE.with(|h| h.set(Some(time)));
            return true;
        }
        HOLDING_FAKE.with(|h| h.set(None));
    }
    if down && vk == b'R' as u16 && (is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C))) {
        RUN_BOX_AT.with(|r| r.set(Some(time)));
        return false;
    }
    let armed = RUN_BOX_AT
        .with(|r| r.get())
        .is_some_and(|at| time.wrapping_sub(at) < RUN_BOX_WINDOW_MS);
    if armed && down && FAST_RUN.with(|f| f.get().1) >= FAKE_RUN {
        RUN_BOX_AT.with(|r| r.set(None));
        HOLDING_FAKE.with(|h| h.set(Some(time)));
        trace_note("fake keyboard: keys held back");
        diag::note("fake keyboard: keys held back", &[]);
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastFakeKeyboard));
        return true;
    }
    false
}

/// The word just ended (`len` characters, then its boundary key) came in
/// one machine-fast burst.
fn came_in_a_burst(len: usize) -> bool {
    len >= SCANNER_MIN && FAST_RUN.with(|f| f.get().1) >= len
}

static SHORTCUTS_ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn shortcuts_in_english() -> bool {
    SHORTCUTS_ENGLISH.load(Ordering::Relaxed)
}
pub fn set_shortcuts_in_english(on: bool) {
    SHORTCUTS_ENGLISH.store(on, Ordering::Relaxed);
}

thread_local! {
    /// The window switched to English for a Ctrl/Alt shortcut, to switch
    /// back to Thai when the keys are let go.
    static SHORTCUT_ENGLISH_IN: std::cell::Cell<Option<isize>> = const { std::cell::Cell::new(None) };
}

fn is_ctrl_or_alt(vk: u16) -> bool {
    matches!(vk, 0x11 | 0x12 | 0xA2..=0xA5)
}

/// Ctrl or Alt pressed with the Thai keyboard on: English until they are
/// let go, so the letter of the shortcut is a Latin letter (apps and web
/// pages that read the character saw Ctrl+แ for Ctrl+C). The request is
/// posted to the app now, and an app takes posted messages before its next
/// key, so it lands before the letter.
unsafe fn shortcut_keys_pressed(vk: u16) {
    if !is_ctrl_or_alt(vk)
        || !shortcuts_in_english()
        || SHORTCUT_ENGLISH_IN.with(|s| s.get()).is_some()
        || policy::supported_layout_id(layout_id(effective_layout()))
            != Some(policy::InputLayout::ThaiKedmanee)
    {
        return;
    }
    SHORTCUT_ENGLISH_IN.with(|s| s.set(Some(GetForegroundWindow().0 as isize)));
    trace_note("shortcut keys: English");
    activate_layout(policy::InputLayout::UsQwerty);
}

/// Ctrl or Alt let go: back to Thai once neither is held, in the same
/// window (Alt+Tab ends in another one, left as it is).
unsafe fn shortcut_keys_released(vk: u16) {
    let Some(hwnd) = SHORTCUT_ENGLISH_IN.with(|s| s.get()) else {
        return;
    };
    if !is_ctrl_or_alt(vk) {
        return;
    }
    // The key being let go still reads as down until this hook returns.
    let still = [0xA2u16, 0xA3, 0xA4, 0xA5]
        .iter()
        .any(|&k| k != vk && is_down(VIRTUAL_KEY(k)));
    if still {
        return;
    }
    SHORTCUT_ENGLISH_IN.with(|s| s.set(None));
    if GetForegroundWindow().0 as isize == hwnd {
        trace_note("shortcut keys: back to Thai");
        activate_layout(policy::InputLayout::ThaiKedmanee);
    }
}

/// The typing practice window, while it is open (0: none).
pub static PRACTICE_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// Posted to the practice window per key pressed: wParam the character its
/// place types on a US keyboard (with Shift as held; 0 for none), lParam
/// the virtual key.
pub const WM_PRACTICE_KEY: u32 = 0x8000 + 0x5A0;

unsafe fn practice_takes(kb: &KBDLLHOOKSTRUCT, msg: u32) -> bool {
    let window = PRACTICE_WINDOW.load(Ordering::Acquire);
    if window == 0 || GetForegroundWindow().0 as isize != window {
        return false;
    }
    let vk = kb.vkCode as u16;
    if is_down(VK_MENU) || is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C)) {
        return false;
    }
    if is_modifier(vk) || matches!(vk, 0x12 | 0x5B | 0x5C | 0xA4 | 0xA5) {
        return false;
    }
    if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
        let shift = is_down(VK_SHIFT);
        let us = righttype::keyboard::index_of(kb.scanCode as u16, kb.flags.0 & 0x01 != 0)
            .and_then(|i| righttype::keyboard::KEYS[i].us)
            .or((vk == VK_SPACE.0).then_some(' '))
            .map(|c| {
                if shift {
                    righttype::trainer::shifted(c).unwrap_or(c)
                } else {
                    c
                }
            });
        let _ = PostMessageW(
            HWND(window as *mut c_void),
            WM_PRACTICE_KEY,
            WPARAM(us.map_or(0, |c| c as usize)),
            LPARAM(vk as isize),
        );
    }
    true
}

static PRACTICE_KEEPS_SCORES: AtomicBool = AtomicBool::new(false);

pub fn practice_keeps_scores() -> bool {
    PRACTICE_KEEPS_SCORES.load(Ordering::Relaxed)
}
pub fn set_practice_keeps_scores(on: bool) {
    PRACTICE_KEEPS_SCORES.store(on, Ordering::Relaxed);
}

static CTRL_HOLD_SHEET: AtomicBool = AtomicBool::new(false);

pub fn ctrl_hold_opens_sheet() -> bool {
    CTRL_HOLD_SHEET.load(Ordering::Relaxed)
}
pub fn set_ctrl_hold_opens_sheet(on: bool) {
    CTRL_HOLD_SHEET.store(on, Ordering::Relaxed);
}

/// Ctrl held this long on its own opens the app's shortcut list.
const CTRL_HOLD: Duration = Duration::from_millis(1000);

thread_local! {
    /// When Ctrl went down on its own (any other key or a mouse button
    /// since cancels it).
    static CTRL_ALONE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// Ctrl, another key, a mouse button or the wheel: Ctrl held alone for
/// [`CTRL_HOLD`] opens the list; anything else in between cancels it
/// (Ctrl+scroll to zoom, Ctrl+click).
pub fn ctrl_hold_input(vk: Option<u16>, down: bool, repeat: bool) {
    let is_ctrl = matches!(vk, Some(0x11 | 0xA2 | 0xA3));
    if is_ctrl && down && !repeat && ctrl_hold_opens_sheet() {
        CTRL_ALONE.with(|c| c.set(Some(Instant::now())));
        unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(None, id);
            let held = CTRL_ALONE
                .with(|c| c.take())
                .is_some_and(|at| at.elapsed() >= CTRL_HOLD - Duration::from_millis(50));
            if held && (is_down(VK_CONTROL)) && !crate::sheet::is_open() {
                trace_note("Ctrl held: shortcut list");
                crate::sheet::request_open();
            }
        }
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(
                None,
                0,
                CTRL_HOLD.as_millis() as u32,
                Some(fire),
            );
        }
    } else if !(is_ctrl && down) {
        CTRL_ALONE.with(|c| c.set(None));
    }
}

thread_local! {
    /// Shift tapped twice opens the list at the text cursor (2.4 A1).
    static SHIFT_TAPS: RefCell<righttype::summon::DoubleTap> =
        RefCell::new(righttype::summon::DoubleTap::new());
}

/// A key (or, with `None`, a mouse button) for the Shift double tap. Not
/// in password fields, apps on the safety list, or RightType's own windows.
pub fn shift_tap_input(vk: Option<u16>, down: bool) {
    use righttype::summon::Input;
    if !crate::caretlist::enabled() {
        return;
    }
    let input = match vk {
        Some(0x10 | 0xA0 | 0xA1) if down => Input::ShiftDown,
        Some(0x10 | 0xA0 | 0xA1) => Input::ShiftUp,
        _ => Input::Other,
    };
    let now = START.get_or_init(Instant::now).elapsed().as_millis() as u64;
    if !SHIFT_TAPS.with(|t| t.borrow_mut().input(input, now)) {
        return;
    }
    let private = STATE.with(|s| s.try_borrow().map_or(true, |s| s.sensitive_app))
        || unsafe { safety::is_password_field() }
        || crate::focus::is_password_field();
    if private || crate::caretlist::is_open() || crate::palette::is_open() {
        return;
    }
    trace_note("Shift twice: list at the cursor");
    crate::caretlist::request_open();
}

static START: OnceLock<Instant> = OnceLock::new();

static HOLD_FOR_ACCENTS: AtomicBool = AtomicBool::new(false);

pub fn holds_for_accents() -> bool {
    HOLD_FOR_ACCENTS.load(Ordering::Relaxed)
}
pub fn set_holds_for_accents(on: bool) {
    HOLD_FOR_ACCENTS.store(on, Ordering::Relaxed);
}

/// The list open near the cursor after a key was held: the key, what it
/// offers, and when it opened.
struct Pick {
    vk: u16,
    choices: Vec<String>,
    opened: Instant,
}

thread_local! {
    static PICK: RefCell<Option<Pick>> = const { RefCell::new(None) };
}

/// The list goes away by itself after this long.
const PICK_OPEN: Duration = Duration::from_secs(6);

/// Holding a key that has other characters (`.` → … · •, `e` → é è, a
/// digit → its Thai numeral) opens a numbered list near the cursor at its
/// first auto-repeat; a digit picks (replacing the one character typed),
/// Esc closes it, any other key closes it and goes on as usual. `Some`
/// when the key is decided here (`true`: swallowed).
unsafe fn hold_to_pick(vk: u16, scan: u16, repeat: bool) -> Option<bool> {
    let open = PICK.with(|p| {
        p.borrow()
            .as_ref()
            .map(|p| (p.vk, p.choices.len(), p.opened.elapsed() < PICK_OPEN))
    });
    if let Some((held, count, fresh)) = open {
        if !fresh {
            PICK.with(|p| p.borrow_mut().take());
        } else if vk == held && repeat {
            return Some(true);
        } else {
            let digit = match vk {
                0x31..=0x39 => Some((vk - 0x31) as usize),
                0x61..=0x69 => Some((vk - 0x61) as usize),
                _ => None,
            };
            let pick = PICK.with(|p| p.borrow_mut().take());
            crate::overlay::dismiss();
            if vk == VK_ESCAPE.0 {
                return Some(true);
            }
            if let (Some(i), Some(pick)) = (digit.filter(|i| *i < count), pick) {
                trace_note("held key: character picked");
                // The character typed by the first press goes; the pick
                // takes its place. What is on screen is no longer the word
                // the buffer holds.
                inject::apply(1, &pick.choices[i], None);
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.buf.clear();
                    st.owned = None;
                    st.mark = TokenMark::Plain;
                    st.recent.clear();
                });
                return Some(true);
            }
            return None;
        }
    }
    if !repeat
        || !holds_for_accents()
        || is_down(VK_CONTROL)
        || is_down(VK_MENU)
        || STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
        || !crate::focus::is_text_field()
    {
        return None;
    }
    let typed = translate(vk, scan)?;
    // The same sets Right Alt steps through (the typist's own included).
    let choices = Some(char_sets().of(typed)).filter(|c| !c.is_empty())?;
    let anchor = crate::caret::find_caret()
        .map_or(crate::overlay::Anchor::Corner, crate::overlay::Anchor::Near);
    crate::overlay::show_at(&righttype::accents::shown(&choices), anchor);
    trace_note("held key: choices shown");
    PICK.with(|p| {
        *p.borrow_mut() = Some(Pick {
            vk,
            choices,
            opened: Instant::now(),
        })
    });
    Some(true)
}

/// The most one key may spend in the hook, waits included. Windows lets a
/// key through by itself when the hook takes longer than it allows
/// (`LowLevelHooksTimeout`, 300 ms or more) and removes a hook that does so
/// often; every wait on the way (focus answers, text boxes, the pause
/// between deletions and text) takes from this budget, so their sum stays
/// inside it.
const HOOK_BUDGET: Duration = Duration::from_millis(200);

thread_local! {
    /// When the key being handled must be done by.
    static HOOK_DEADLINE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// How much of the key's [`HOOK_BUDGET`] is left; `max` outside the hook.
pub fn budget_left(max: Duration) -> Duration {
    HOOK_DEADLINE.with(|d| d.get()).map_or(max, |at| {
        at.saturating_duration_since(Instant::now()).min(max)
    })
}

thread_local! {
    /// The hook is handling a key (see the re-entry note in the hook).
    static PROCESSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A key arrived while one was being handled.
    static NESTED_KEY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The key being handled and those passed through while it was:
    /// `(virtual key, time stamp, message)`.
    static SEEN: RefCell<Vec<(u32, u32, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Computer-driven Windows E2E necessarily uses `SendInput`, which Windows marks
/// as injected. A debug build may opt into processing those events so the real
/// hook pipeline can be exercised. Release builds compile this escape hatch to
/// `false` and always ignore third-party injected input.
fn debug_e2e_accepts_injected() -> bool {
    #[cfg(debug_assertions)]
    {
        static ACCEPT: OnceLock<bool> = OnceLock::new();
        *ACCEPT.get_or_init(|| std::env::var_os("RIGHTTYPE_E2E_ACCEPT_INJECTED").is_some())
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

#[cfg(debug_assertions)]
pub(crate) fn e2e_trace(msg: String) {
    if debug_e2e_accepts_injected() {
        eprintln!("[rt-e2e] {msg}");
    }
}

#[cfg(not(debug_assertions))]
pub(crate) fn e2e_trace(_: String) {}

/// The keyboard the focused app types with now, if RightType has a table for
/// it (the tray's TH / EN icon).
pub fn current_language() -> Option<policy::InputLayout> {
    unsafe { policy::supported_layout_id(layout_id(effective_layout())) }
}

/// The program the typist is typing in (its file name), as last seen.
pub(crate) fn current_app() -> Option<String> {
    if let Some(exe) = STATE.with(|s| s.try_borrow().ok().and_then(|s| s.app_exe.clone())) {
        return Some(exe);
    }
    unsafe {
        let fg = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if !fg.0.is_null() {
            safety::foreground_exe(fg)
        } else {
            None
        }
    }
}

/// A fixed message for both the debug trace and the problem report
/// ([`diag`]); `'static`, so it cannot carry typed text.
pub(crate) fn trace_note(msg: &'static str) {
    e2e_trace(msg.to_string());
    diag::note(msg, &[]);
}

/// Debug e2e builds: report a fatal exception (code, address and the
/// faulting thread's stack) to stderr before Windows ends the process, which
/// otherwise dies with only an exit code (`0xC000041D` when it happens
/// inside a callback Windows made into us). First-chance, so some reported
/// access violations may be ones a system DLL catches itself; the last
/// report before the process ends is the one that killed it.
#[cfg(debug_assertions)]
pub fn report_fatal_exceptions() {
    use windows::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_POINTERS,
    };
    unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
        static REPORTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
        let Some(record) = info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) else {
            return EXCEPTION_CONTINUE_SEARCH;
        };
        let code = record.ExceptionCode.0 as u32;
        let fatal = matches!(
            code,
            0xC000_0005 // access violation
                | 0xC000_001D // illegal instruction
                | 0xC000_0096 // privileged instruction
                | 0xC000_0374 // heap corruption
                | 0xC000_0409 // stack buffer overrun / fail fast
                | 0xC000_00FD // stack overflow
        );
        if !fatal || REPORTS.fetch_add(1, Ordering::Relaxed) >= 5 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        eprintln!(
            "[rt-e2e] EXCEPTION {code:#010x} at {:?} (info {:x?})",
            record.ExceptionAddress,
            &record.ExceptionInformation[..record.NumberParameters.min(3) as usize]
        );
        // Too little stack left to walk it after an overflow.
        if code != 0xC000_00FD {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
        EXCEPTION_CONTINUE_SEARCH
    }
    if debug_e2e_accepts_injected() {
        unsafe {
            AddVectoredExceptionHandler(1, Some(on_exception));
        }
    }
}

fn is_modifier(vk: u16) -> bool {
    matches!(
        vk,
        v if v == VK_SHIFT.0
            || v == 0xA0 // VK_LSHIFT
            || v == 0xA1 // VK_RSHIFT
            || v == VK_CONTROL.0
            || v == 0xA2 // VK_LCONTROL
            || v == 0xA3 // VK_RCONTROL
            || v == VK_MENU.0
            || v == 0xA4 // VK_LMENU
            || v == 0xA5 // VK_RMENU
            || v == VK_CAPITAL.0
    )
}

unsafe fn is_layout_switch_trigger(vk: u16) -> bool {
    // 1. Grave Accent (VK_OEM_3 = 0xC0)
    if vk == 0xC0 {
        return true;
    }
    // 2. Win + Space
    if vk == VK_SPACE.0 && (is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C))) {
        // VK_LWIN = 0x5B, VK_RWIN = 0x5C
        return true;
    }
    // 3. Alt + Shift / Ctrl + Shift (when one of them is pressed while the other modifier is held)
    let is_shift = vk == VK_SHIFT.0 || vk == 0xA0 || vk == 0xA1;
    let is_menu = vk == VK_MENU.0 || vk == 0xA4 || vk == 0xA5;
    let is_ctrl = vk == VK_CONTROL.0 || vk == 0xA2 || vk == 0xA3;

    if (is_shift && (is_down(VK_MENU) || is_down(VK_CONTROL)))
        || (is_menu && is_down(VK_SHIFT))
        || (is_ctrl && is_down(VK_SHIFT))
    {
        return true;
    }

    false
}

/// Process one event. Returns `true` to swallow the current key (we handled a
/// hotkey or corrected a word and re-injected its boundary), `false` to let it
/// pass through normally.
unsafe fn process(msg: u32, kb: &KBDLLHOOKSTRUCT) -> bool {
    let vk = kb.vkCode as u16;
    let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    // Windows repeats a held key as more key-downs; only a release ends it.
    let repeat = if down {
        HELD_KEY.swap(vk, Ordering::Relaxed) == vk
    } else {
        let _ = HELD_KEY.compare_exchange(vk, 0, Ordering::Relaxed, Ordering::Relaxed);
        false
    };
    ctrl_hold_input(Some(vk), down, repeat);
    shift_tap_input(Some(vk), down);
    // Right Alt tapped on its own steps the last character through its set.
    if vk == 0xA5 {
        if down && !repeat {
            RALT_DOWN.with(|r| r.set(Some(Instant::now())));
        } else if !down {
            let tap = RALT_DOWN
                .with(|r| r.take())
                .is_some_and(|at| at.elapsed() < RALT_TAP);
            if tap && cycle_character() {
                // The release goes to the app masked: no menu bar.
                inject::release_right_alt_masked();
                return true;
            }
        }
    } else if down {
        RALT_DOWN.with(|r| r.set(None));
        if !is_modifier(vk) {
            // Esc puts the character back; any other key keeps it.
            if vk == VK_ESCAPE.0 && cycle_back() {
                return true;
            }
            CYCLE.with(|c| c.borrow_mut().take());
        }
    }
    if !down {
        if vk == VK_BACK.0 {
            FLIP_DOWN.with(|f| f.set(None));
        }
        shortcut_keys_released(vk);
        if vk == VK_CAPITAL.0 {
            if let Some(at) = CAPS_DOWN_AT.with(|c| c.take()) {
                caps_released(at);
                return true;
            }
        }
        return false;
    }
    e2e_trace(format!("key vk={vk:#x} repeat={repeat}"));
    // Shift+Backspace still held: its repeats are the flip's, with or
    // without Shift. A flip sends Shift-up with its keys, so the next
    // repeats arrive as plain Backspace and deleted the words just flipped
    // (CI: `l;ylfu 8iy[`). Swallowed until Backspace is released.
    if vk == VK_BACK.0 && repeat && FLIP_DOWN.with(|f| f.get()).is_some() {
        flip_held_repeat();
        return true;
    }
    if capture_key(vk) {
        return true;
    }
    if !repeat {
        shortcut_keys_pressed(vk);
    }
    if let Some(swallow) = hold_to_pick(vk, kb.scanCode as u16, repeat) {
        return swallow;
    }
    let action = hotkeys().action_for(vk, is_down(VK_CONTROL), is_down(VK_SHIFT), is_down(VK_MENU));
    if !is_modifier(vk) && is_down(VK_SHIFT) && (is_down(VK_CONTROL) || is_down(VK_MENU)) {
        let hkl = STATE.with(|s| s.borrow().last_hkl);
        SHORTCUT.with(|c| c.set(Some((Instant::now(), hkl))));
    }

    // The command palette is open and in front: its keys are its own
    // (arrows, Enter, 1–9, typing to search, Esc), so it works without a
    // mouse. Its own hotkey still closes it.
    if action != Some(Action::Palette)
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && crate::palette::is_open()
    {
        let ch = translate(vk, kb.scanCode as u16);
        if crate::palette::key(vk, ch) {
            return true;
        }
    }
    if !is_down(VK_CONTROL) && !is_down(VK_MENU) {
        let ch = translate(vk, kb.scanCode as u16);
        if crate::sheet::key(vk, ch) || crate::caretlist::key(vk, ch) {
            return true;
        }
    }
    // Ctrl+P in the list at the cursor: pin the character to a key.
    if vk == u16::from(b'P')
        && is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && crate::caretlist::is_open()
        && crate::caretlist::pin_start()
    {
        return true;
    }

    // Tab (alone) takes a Thai completion on offer; any other key drops it.
    let completion = COMPLETION.with(|c| c.borrow_mut().take());
    if let Some(offer) = completion {
        let fresh = offer.hwnd == GetForegroundWindow().0 as isize
            && offer.focus_generation == crate::focus::generation()
            && offer.created.elapsed() < COMPLETION_OPEN;
        if vk == VK_TAB.0
            && fresh
            && !is_down(VK_SHIFT)
            && !is_down(VK_CONTROL)
            && !is_down(VK_MENU)
            && !crate::focus::is_password_field()
        {
            crate::overlay::dismiss();
            if inject::apply(0, &offer.rest, None) {
                trace_note("Thai completion taken with Tab");
                // The word is whole and right: nothing left to decide.
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.buf.clear();
                    st.mark = TokenMark::Plain;
                });
                return true;
            }
        } else if !is_modifier(vk) {
            crate::overlay::dismiss();
        }
    }

    // Tab (alone) takes a ghost suggestion on offer; any other key drops it.
    let ghost = GHOST.with(|g| g.borrow_mut().take());
    if let Some(offer) = ghost {
        let fresh = offer.hwnd == GetForegroundWindow().0 as isize
            && offer.focus_generation == crate::focus::generation()
            && offer.created.elapsed() < Duration::from_millis(crate::overlay::OFFER_MS.into());
        if vk == VK_TAB.0
            && fresh
            && !is_down(VK_SHIFT)
            && !is_down(VK_CONTROL)
            && !is_down(VK_MENU)
            && !safety::is_password_field()
            && !crate::focus::is_password_field()
        {
            crate::overlay::dismiss();
            if inject::apply(offer.replace, &offer.text, None) {
                trace_note("ghost suggestion taken with Tab");
                ghost_tail_forget();
                // What was typed is a symbol now: no word to decide on.
                word_overwritten();
                return true;
            }
        } else if !is_modifier(vk) {
            crate::overlay::dismiss();
        }
    }

    // Tab (alone) right after a Suggest hint takes it, like Alt+CapsLock. The
    // hint exists only until the next key, so Tab is otherwise untouched.
    // This runs ahead of the context checks below, so it checks for itself
    // that the caret is still where the hint was made: same window, same
    // focused field (a Tab that ended the word may have moved focus, so a
    // hint made at a Tab boundary is never taken by Tab), and not a password
    // field.
    if vk == VK_TAB.0
        && !is_down(VK_SHIFT)
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && STATE.with(|s| {
            s.borrow().suggestion.as_ref().is_some_and(|x| {
                x.created.elapsed() < SUGGEST_TAB_WINDOW
                    && x.boundary_vk != VK_TAB.0
                    && x.hwnd == GetForegroundWindow().0 as isize
                    && x.focus_generation == crate::focus::generation()
            })
        })
        && !safety::is_password_field()
        && !crate::focus::is_password_field()
    {
        accept_suggestion();
        return true;
    }
    // Tab while a hint is showing for the word still being typed: flip it now.
    if vk == VK_TAB.0
        && !is_down(VK_SHIFT)
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && STATE.with(|s| {
            let st = s.borrow();
            !st.buf.current().is_empty()
                && st.live_hint.is_some_and(|h| {
                    h.created.elapsed() < SUGGEST_TAB_WINDOW
                        && h.hwnd == GetForegroundWindow().0 as isize
                        && h.focus_generation == crate::focus::generation()
                })
        })
        && !safety::is_password_field()
        && !crate::focus::is_password_field()
    {
        STATE.with(|s| s.borrow_mut().live_hint = None);
        crate::overlay::dismiss();
        convert_last_word();
        return true;
    }

    // The flip hotkey (Shift+Backspace by default) keeps the recent words.
    let is_flip = action == Some(Action::Flip);
    if !is_flip && action.is_none() && !is_modifier(vk) {
        // Keys that edit or move away from the text before the caret, and any
        // command chord, make the recent words stale. Typing (a character or
        // a boundary) keeps them: it only adds after them.
        let stale = is_down(VK_CONTROL) || is_down(VK_MENU) || moves_or_edits(vk);
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.recent.end_chain();
            if stale {
                st.recent.clear();
            }
            st.suggestion = None;
            st.live_hint = None;
        });
    } else if (action.is_some() && !is_flip) || vk == VK_CAPITAL.0 {
        // Other hotkeys (convert selection, undo, ...) can rewrite text, and
        // CapsLock alone changes what the next keys type.
        // (The pending suggestion stays: the Accept hotkey is one of these.)
        STATE.with(|s| s.borrow_mut().recent.clear());
    }

    // Panic switch (Ctrl+Alt+CapsLock by default) instantly flips master
    // enable, either way. Checked before the enabled gate and the
    // sensitive-context guard below so it always works — including turning
    // back ON, and even from inside a password field or blacklisted app.
    if action == Some(Action::Panic) {
        let now_on = !ENABLED.fetch_xor(true, Ordering::Relaxed);
        crate::overlay::show(righttype::i18n::tr(if now_on {
            righttype::i18n::T::ToastOn
        } else {
            righttype::i18n::T::ToastOff
        }));
        crate::config::persist_async();
        return true;
    }

    // Master switch: when disabled, pass everything through untouched.
    if !ENABLED.load(Ordering::Relaxed) {
        e2e_trace("key passed through: disabled".to_string());
        return false;
    }

    // Ctrl+CapsLock: cycle Manual → Auto → Suggest. Swallow so Caps never
    // flips. Handled ahead of the context guards: it never touches text, and
    // Electron apps (Claude, VS Code, Slack, Discord) frequently report a UIA
    // focus we cannot classify, which the guards below must treat as a
    // password field — so the chord used to fall through there and silently
    // toggle CapsLock instead of switching mode.
    if action == Some(Action::Cycle) {
        cycle_mode();
        return true;
    }

    // The keyboard map types only what is clicked on it: it opens everywhere.
    if action == Some(Action::KeyMap) {
        if !repeat {
            crate::keymap::request_toggle();
        }
        return true;
    }

    // The command palette never touches text either: it opens everywhere.
    // Opened after this callback returns, never inside the hook.
    if action == Some(Action::Palette) {
        if !repeat {
            crate::palette::request_open();
        }
        return true;
    }

    // A user-initiated layout switch invalidates buffered caret context. Windows
    // performs the switch itself; we only discard state here.
    //
    // This fires on the *modifier* key-down, before we can tell a layout switch
    // from the Ctrl+Shift prefix of RightType's own Undo hotkey. A run we own
    // must therefore be withdrawn rather than abandoned: dropping it would leave
    // rendered Thai on screen with nothing tracking it, and abandoning it while
    // clearing the buffer would make the next reconcile delete text it no longer
    // has a run for. Withdrawing is right under either reading of the chord.
    if is_layout_switch_trigger(vk) {
        withdraw_owned_run();
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.pending_hkl = None;
            st.buf.clear();
            st.mark = TokenMark::Plain;
            st.recent.clear();
        });
    }

    // If focus or layout changed since the last key, the buffered word is stale
    // (the focus worker's answer about this key's field first).
    crate::focus::settle(Duration::from_millis(60));
    sync_context();
    note_keyboard(effective_layout());

    // Never run where secrets are typed: blacklisted apps, or password fields
    // (native ES_PASSWORD, or UIA-detected ones in browsers/Electron/UWP).
    if STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_full_screen()
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        e2e_trace(format!(
            "key passed through: sensitive_app={} native_password={} uia_protected={}",
            STATE.with(|s| s.borrow().sensitive_app),
            safety::is_password_field(),
            crate::focus::is_password_field()
        ));
        STATE.with(|s| s.borrow_mut().suggestion = None);
        ghost_forget();
        return false;
    }
    // Switched off in this app (its per-app mode): touch nothing, like a
    // blocked app, but the hotkeys above (on/off, mode cycle) still work.
    let Some(mode_now) = mode_here() else {
        e2e_trace("key passed through: off in this app".to_string());
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.buf.clear();
            st.suggestion = None;
            st.live_hint = None;
            st.recent.clear();
        });
        ghost_forget();
        return false;
    };

    // The grave key types its character (`, ~, or _ % on the Thai keyboard)
    // instead of switching the language, when the typist asked for that:
    // Windows' Thai setup makes it the language key, and code and Markdown
    // need it.
    if vk == 0xC0 && grave_types() && action.is_none() && !is_down(VK_CONTROL) && !is_down(VK_MENU)
    {
        let us = if is_down(VK_SHIFT) { "~" } else { "`" };
        let thai = policy::supported_layout_id(layout_id(effective_layout()))
            == Some(policy::InputLayout::ThaiKedmanee);
        let ch = if thai {
            righttype::layout::en_to_th(us)
        } else {
            us.to_string()
        };
        STATE.with(|s| s.borrow_mut().buf.clear());
        if inject::apply(0, &ch, None) {
            return true;
        }
    }

    // Keys that change how the next keys type, without a sign of it: the
    // numeric keypad with NumLock off (it moves the caret instead of typing
    // digits), and Insert (overtype in the apps that have it). Only in a text
    // field, and only plain presses.
    if action.is_none()
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && !repeat
        && crate::focus::is_text_field()
    {
        let extended = kb.flags.0 & 0x01 != 0;
        if let Some(digit) = keypad_digit(vk, extended) {
            if !is_down(VK_SHIFT) && GetKeyState(0x90) & 1 == 0 {
                match numlock_mode() {
                    KeyGuard::Fix => {
                        // NumLock on, and the digit the typist meant.
                        inject::toggle_numlock();
                        let mut s = [0u8; 4];
                        if inject::apply(0, digit.encode_utf8(&mut s), None) {
                            trace_note("keypad with NumLock off: turned it on");
                            crate::overlay::badge_at_caret("NUM");
                            return true;
                        }
                    }
                    KeyGuard::Warn => {
                        warn_once(&NUM_WARNED, "NUM", righttype::i18n::T::ToastNumLockOff)
                    }
                    KeyGuard::Off => {}
                }
            }
        } else if vk == VK_INSERT.0 && extended && !is_down(VK_SHIFT) {
            match insert_mode() {
                KeyGuard::Fix => {
                    trace_note("Insert held back in a text field");
                    crate::overlay::show(righttype::i18n::tr(
                        righttype::i18n::T::ToastInsertBlocked,
                    ));
                    return true;
                }
                KeyGuard::Warn => {
                    trace_note("Insert pressed in a text field: said so");
                    crate::overlay::show(righttype::i18n::tr(
                        righttype::i18n::T::ToastInsertPressed,
                    ))
                }
                KeyGuard::Off => {}
            }
        }
    }

    // Ctrl+Backspace after Thai: one Thai word, not the whole run (Thai has
    // no spaces between words, so Windows takes everything back to the last
    // space). Only when the text before the caret says so; otherwise the key
    // is Windows' own.
    if vk == VK_BACK.0
        && action.is_none()
        && is_down(VK_CONTROL)
        && !is_down(VK_SHIFT)
        && !is_down(VK_MENU)
        && deletes_thai_words()
        && STATE.with(|s| s.borrow().owned.is_none())
        // Browsers, Electron apps and Office already delete one Thai word.
        && !current_app().is_some_and(|e| righttype::compat::breaks_thai_words(&e))
    {
        let before = crate::focus::text_before_caret_within(80, Duration::from_millis(60));
        let n = before
            .as_ref()
            .and_then(|t| righttype::segment::last_word_to_delete(t, righttype::dict::thai()));
        drop(before);
        e2e_trace(format!("ctrl+backspace: thai word of {n:?} characters"));
        if let Some(n) = n {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.buf.clear();
                st.mark = TokenMark::Plain;
                st.recent.clear();
                st.undo = None;
            });
            // The Backspaces go without Ctrl (`apply` lets go of it, or the
            // app would take a word for each); the typist still holds it.
            if inject::apply(n, "", None) {
                inject::hold_again(VK_CONTROL.0);
                return true;
            }
        }
    }

    // Enter in a chat app, on a message typed on the wrong keyboard: held
    // once, so it is not sent unreadable (Enter again sends it).
    if vk == VK_RETURN.0 && action.is_none() && !repeat {
        if !is_down(VK_SHIFT)
            && !is_down(VK_CONTROL)
            && !is_down(VK_MENU)
            && STATE.with(|s| s.borrow().owned.is_none())
            && hold_enter(mode_now)
        {
            return true;
        }
    } else if !is_modifier(vk) {
        // Any other key: the next Enter is looked at afresh.
        ENTER_HELD.with(|h| h.set(None));
    }

    // The text hotkeys (CapsLock chords by default).
    {
        if action == Some(Action::Undo) {
            // Undo the last correction (one-shot). Swallow.
            e2e_trace("undo-hotkey received".to_string());
            // A run we still own is the most recent correction there is, and it
            // has no Undo record yet (that is written when the run anchors), so
            // withdrawing our rendering *is* the undo.
            if withdraw_owned_run() {
                note_rejection();
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    // Keep the token but mark it decided: without this the very
                    // next keystroke re-evaluates the same text and can
                    // immediately re-apply the reading the typist just rejected.
                    st.mark = TokenMark::Decided { learn: true };
                    st.undo = None;
                });
                crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
            } else if !manual::request_undo_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            ) {
                undo_last_correction();
            }
            return true;
        }
        if action == Some(Action::Accept) {
            accept_suggestion();
            return true;
        }
        if action == Some(Action::Selection) {
            // Convert the current selection. Swallow.
            e2e_trace("convert-selection-hotkey received".to_string());
            manual::request_convert_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            );
            return true;
        }
        // CapsLock alone (or a chord that is not a hotkey) is a normal toggle.
        // A word is kept as if CapsLock were off and shown with its state,
        // so one typed across a toggle is left alone.
        if vk == VK_CAPITAL.0 && caps_switches_language() && is_enabled() {
            // A language key now: the release decides (tap or hold).
            if !repeat {
                CAPS_DOWN_AT.with(|c| c.set(Some(Instant::now())));
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.buf.clear();
                    st.owned = None;
                    st.mark = TokenMark::Plain;
                });
            }
            return true;
        }
        if vk == VK_CAPITAL.0 {
            // Turning it on: say so where the eyes are, before a sentence
            // comes out in capitals (the state flips after this key).
            if !caps_on() && crate::caret::is_enabled() {
                crate::overlay::badge_at_caret("CAPS");
            }
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.buf.clear();
                st.owned = None;
                st.mark = TokenMark::Plain;
            });
            return false;
        }
    }

    // Shift+Backspace: flip the current word in place. Always swallowed — even
    // when there's nothing to convert — so the key's auto-repeat can't fall
    // through to a destructive Backspace and delete the result we just injected.
    if is_flip {
        e2e_trace(format!(
            "flip: repeat={repeat} buf={} recent={}",
            STATE.with(|s| s.borrow().buf.current().chars().count()),
            STATE.with(|s| s.borrow().recent.len()),
        ));
        if !repeat {
            diag::note(
                "Shift+Backspace",
                &[
                    (
                        "word_in_progress",
                        STATE
                            .with(|s| s.borrow().buf.current().chars().count())
                            .into(),
                    ),
                    (
                        "recent_words",
                        STATE.with(|s| s.borrow().recent.len()).into(),
                    ),
                ],
            );
        }
        // Holding the keys flips the rest of the run in one go, once (after
        // FLIP_HOLD, timed from the press: the repeat delay is the typist's
        // own setting); the other repeats do nothing, or auto-repeat would
        // run back and forth through the words.
        if repeat {
            flip_held_repeat();
            return true;
        }
        FLIP_DOWN.with(|f| f.set(Some((Instant::now(), false))));
        // While we own the run the screen does not match the buffer, so the
        // manual path's backspace count would be wrong. Withdraw our rendering
        // first; the typist asked for the raw keystrokes back.
        if withdraw_owned_run() {
            note_rejection();
            // The typist rejected our reading mid-word. Leave the rest of this
            // token alone, and learn it once it is complete.
            STATE.with(|s| s.borrow_mut().mark = TokenMark::Decided { learn: true });
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
            return true;
        }
        convert_last_word();
        return true;
    }

    let Some(key) = classify(vk, kb.scanCode as u16) else {
        e2e_trace(format!("key vk={vk:#x} not classified"));
        // Something we cannot follow (a dead key, a function key): the text
        // before the caret may not be what we recorded. A modifier pressed on
        // its own types nothing — and Shift is how Shift+Backspace starts, so
        // clearing here made it forget the word it was pressed to flip.
        if !is_modifier(vk) {
            STATE.with(|s| s.borrow_mut().recent.clear());
            ghost_forget();
        }
        return false;
    };
    ghost_track(key, vk);

    // Backspace right after a spelling fix takes the fix back.
    if key == Key::Backspace
        && SPELLING_AT
            .with(|t| t.take())
            .is_some_and(|at| at.elapsed() < SPELLING_GRACE)
        && STATE.with(|s| {
            s.borrow()
                .undo
                .as_ref()
                .is_some_and(|u| matches!(u.kind, UndoKind::Spelling))
        })
    {
        diag::note("Backspace right after a spelling fix: fix taken back", &[]);
        undo_last_correction();
        return true;
    }
    if matches!(key, Key::Char(_) | Key::Boundary) {
        SPELLING_AT.with(|t| t.set(None));
    }

    // Any text reaching the app moves the caret past a correction's Undo
    // window: the record counts characters from the end, so replaying it now
    // would delete what was just typed instead of what we changed.
    if matches!(key, Key::Char(_) | Key::Backspace | Key::Boundary) {
        STATE.with(|s| s.borrow_mut().undo = None);
    }

    // Drive the buffer; only a boundary can return a completed word.
    let completed = STATE.with(|s| s.borrow_mut().buf.observe(key));
    if completed.is_none() && key == Key::Boundary {
        // A boundary after nothing (a second space) is not in the record.
        STATE.with(|s| s.borrow_mut().recent.clear());
    }
    let Some(mut word) = completed else {
        // D-008 revisable rendering: reconcile the screen with the run's current
        // best reading. Only Char and Backspace change the run, and only a
        // token nobody has decided yet is RightType's to reinterpret.
        // A Backspace re-renders only a run we already own. Before we own
        // one, the screen still holds the character being deleted, so the
        // "run minus the new key" model below would be off by two and leave
        // raw keys mixed into the Thai (`mujouj1⌫` → `muที่นี่`); letting the
        // Backspace through keeps the screen and the buffer in step, and the
        // next key (or the boundary) reads the word again.
        let may_reconcile = match key {
            Key::Char(_) => true,
            Key::Backspace => STATE.with(|s| s.borrow().owned.is_some()),
            _ => false,
        };
        if matches!(key, Key::Char(_))
            && mode_now == Mode::Suggest
            && STATE.with(|s| s.borrow().mark == TokenMark::Plain)
            && policy::supported_layout_id(layout_id(effective_layout()))
                == Some(policy::InputLayout::UsQwerty)
        {
            show_live_hint();
        }
        // A long Thai word on its way: offer the rest, when every word that
        // starts like this goes on the same way (opt-in).
        if matches!(key, Key::Char(_))
            && completes_thai()
            && mode_now != Mode::Code
            && policy::supported_layout_id(layout_id(effective_layout()))
                == Some(policy::InputLayout::ThaiKedmanee)
        {
            offer_completion();
        }
        // In Word the text is tagged with the keyboard language when it
        // arrives, so words are corrected at their boundary only (after the
        // keyboard has been switched), never rewritten key by key.
        if may_reconcile
            && mode_now == Mode::Auto
            && !tags_language_here()
            && STATE.with(|s| s.borrow().mark == TokenMark::Plain)
            && policy::supported_layout_id(layout_id(effective_layout()))
                == Some(policy::InputLayout::UsQwerty)
        {
            if reconcile_run() {
                take_down_preview();
                // The text kept for ghost suggestions followed what was
                // written (`ghost_applied`): the key that finished a word
                // put into Thai can finish math said in words too.
                if ghost_here(key, mode_now) {
                    offer_ghost();
                }
                return true;
            }
            show_preview();
        }
        if ghost_here(key, mode_now) {
            offer_ghost();
        }
        // Navigation and focus events move the caret away from the run, so the
        // text we rendered is no longer ours to edit. Let go without touching it.
        if key == Key::Reset {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.owned = None;
                st.mark = TokenMark::Plain;
            });
        } else {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                // Too long to describe any more, or erased back to nothing:
                // either way nothing about the token is ours any more.
                if st.buf.is_poisoned() || st.buf.current().is_empty() {
                    st.mark = TokenMark::Plain;
                }
            });
        }
        // Everything else passes through; the buffer already tracked it.
        return false;
    };
    let mark = STATE.with(|s| std::mem::replace(&mut s.borrow_mut().mark, TokenMark::Plain));

    // A boundary ends a run we own. If its reading cannot end as Thai and
    // the keys were all letters, they go back to what was typed and the word
    // is judged like any other below (see `policy::run_goes_back`).
    let back = STATE.with(|s| {
        s.borrow().owned.as_ref().is_some_and(|o| {
            policy::run_goes_back(&word, &o.rendered, dict::english(), dict::thai())
        })
    });
    if back {
        e2e_trace(format!("boundary: {word:?} cannot end as Thai, withdrawn"));
        diag::note(
            "word end: live Thai put back to the keys",
            &[("keys", Shape::of(&word).into())],
        );
        withdraw_owned_run_to(&word);
    }
    // Otherwise its reading has already been applied to the screen, so the
    // boundary path must not correct it a second time — its backspace count
    // assumes the screen still holds the raw keystrokes.
    if let Some(mut rendered) = anchor_owned_run(&word, vk) {
        e2e_trace("boundary: owned run anchored".to_string());
        diag::note(
            "word end: live Thai kept",
            &[("shown", Shape::of(&rendered).into())],
        );
        remember_completed(&rendered, vk, true);
        rendered.zeroize();
        word.zeroize();
        return false;
    }

    // The typist settled this token by hand; honour that, and learn the word
    // when what they rejected was RightType's own conversion.
    if let TokenMark::Decided { learn } = mark {
        // Still part of the typed stream: the seed-phrase guard must see it.
        let seed_run = STATE.with(|s| s.borrow_mut().seed.observe_candidate(&word, None));
        if learn && !seed_run {
            crate::learn::learn_now(&word);
        }
        remember_as_shown(&word, vk, false);
        word.zeroize();
        return false;
    }

    let active_layout = policy::supported_layout_id(layout_id(effective_layout())).filter(|l| {
        *l != policy::InputLayout::UsQwerty
            || !THAI_UNREADABLE.load(std::sync::atomic::Ordering::Relaxed)
    });

    // A barcode scanner types like a US keyboard, as fast as a machine:
    // with the Thai keyboard on, `8851234567890` came out as Thai. A burst
    // no finger could type, ended by Enter or Tab, goes back to the
    // scanner's characters, in every mode.
    if fixes_scanners()
        && active_layout == Some(policy::InputLayout::ThaiKedmanee)
        && (vk == VK_RETURN.0 || vk == VK_TAB.0)
        && came_in_a_burst(word.chars().count())
    {
        let mut scanned = righttype::layout::th_to_en(&word);
        if scanned != *word && scanned.chars().all(|c| c.is_ascii_graphic()) {
            e2e_trace(format!("scanner burst put back ({} keys)", scanned.len()));
            diag::note(
                "scanner burst put back",
                &[("length", scanned.len().into())],
            );
            inject::expect_before_caret(&word);
            let done = inject::apply(word.chars().count(), &scanned, Some(vk));
            scanned.zeroize();
            STATE.with(|s| s.borrow_mut().recent.clear());
            word.zeroize();
            return done;
        }
        scanned.zeroize();
    }

    // A snippet's trigger: its text instead, in every mode.
    let snippet = active_layout.and_then(|layout| {
        SNIPPETS
            .read()
            .ok()
            .and_then(|l| righttype::snippets::find(&l, &word, layout).cloned())
    });
    if let Some(mut snippet) = snippet {
        diag::note(
            "snippet expanded",
            &[("text", Shape::of(&snippet.text).into())],
        );
        let done = expand_snippet(&word, vk, &snippet.text);
        snippet.text.zeroize();
        STATE.with(|s| s.borrow_mut().recent.clear());
        word.zeroize();
        // Expanded: the boundary was typed after the text, swallow the key.
        return done;
    }

    let converted = mark == TokenMark::Converted;
    let mut detection = if converted {
        // D-009: the whole token, including the part converted before the
        // anchor, gets its first complete look now.
        policy::revise_converted(&word, dict::english(), dict::thai())
    } else {
        active_layout
            .and_then(|layout| policy::detect_token(&word, layout, dict::english(), dict::thai()))
    };
    e2e_trace(format!(
        "word={word:?} layout={active_layout:?} converted={converted} det={:?} mode={:?}",
        detection.as_ref().map(|d| d.corrected.clone()),
        mode_now
    ));
    diag::note(
        "word end",
        &[
            ("typed", Shape::of(&word).into()),
            (
                "layout",
                match active_layout {
                    Some(policy::InputLayout::UsQwerty) => "English",
                    Some(policy::InputLayout::ThaiKedmanee) => "Thai",
                    None => "other",
                }
                .into(),
            ),
            ("switched_mid_word", converted.into()),
            ("wrong_layout", detection.is_some().into()),
        ],
    );

    // Track the meaningful English stream, including a wrong-layout candidate.
    // This cannot retroactively protect the first words of a phrase (ordinary
    // English overlaps BIP39), but once the run reaches the threshold it blocks
    // Auto, Suggest and learning for the current and following seed words.
    let seed_run = STATE.with(|s| {
        s.borrow_mut()
            .seed
            .observe_candidate(&word, detection.as_ref().map(|d| d.corrected.as_str()))
    });
    if seed_run {
        detection = None;
        forget_recent_text();
    }
    // English put in place of the word is shown as CapsLock shows it.
    if let Some(d) = detection.as_mut() {
        d.corrected = policy::shown_with_caps(&d.corrected, caps_on());
    }

    // Learning sees only ordinary US-QWERTY input for which the production
    // policy found no wrong-layout candidate. This keeps converted candidates
    // and unsupported layouts out of the persistence path.
    if !seed_run && policy::allows_learning(active_layout, detection.is_some()) {
        crate::learn::observe(&word);
    }

    // Code mode (code editors): names stay, Thai only in comments and
    // strings, and Thai keys typed for code come back as the English typed.
    let mut code_hint = false;
    if mode_now == Mode::Code && !seed_run {
        if let Some(layout) = active_layout {
            let line_matters = match layout {
                policy::InputLayout::UsQwerty => {
                    detection.is_some() && !righttype::code::looks_like_identifier(&word)
                }
                policy::InputLayout::ThaiKedmanee => {
                    detection.is_none()
                        && righttype::code::thai_keys_look_like_code(&word, dict::thai())
                }
            };
            let prose = if line_matters {
                crate::focus::text_before_caret_within(160, Duration::from_millis(80))
                    .map(|t| righttype::code::line_is_prose(&t))
            } else {
                None
            };
            let verdict = righttype::code::verdict(
                &word,
                layout,
                detection.as_ref().map(|d| d.corrected.as_str()),
                prose,
                dict::thai(),
            );
            diag::note(
                "code mode",
                &[
                    (
                        "line",
                        match prose {
                            Some(true) => "comment or string",
                            Some(false) => "code",
                            None => "unknown",
                        }
                        .into(),
                    ),
                    (
                        "verdict",
                        match &verdict {
                            righttype::code::Verdict::Fix(_) => "fix",
                            righttype::code::Verdict::Hint(_) => "hint",
                            righttype::code::Verdict::Leave => "leave",
                        }
                        .into(),
                    ),
                ],
            );
            let corrected = match verdict {
                righttype::code::Verdict::Fix(c) => Some(c),
                righttype::code::Verdict::Hint(c) => {
                    code_hint = true;
                    Some(c)
                }
                righttype::code::Verdict::Leave => None,
            };
            detection = corrected.map(|corrected| righttype::detect::Detection {
                corrected,
                confidence: righttype::detect::Confidence::High,
                evidence: righttype::detect::Evidence::ExactDictionary,
            });
        }
    }

    // CapsLock left on by accident (`hELLO`, or Thai typed with every key
    // shifted): in Auto the word is put as meant and CapsLock turned off.
    // Thai typed in a wrong order that looks right (เเ for แ, ํา for ำ, a
    // tone mark before the vowel): put right when that is a Thai word.
    if detection.is_none()
        && !seed_run
        && mode_now == Mode::Auto
        && policy::supported_layout_id(layout_id(effective_layout()))
            == Some(policy::InputLayout::ThaiKedmanee)
    {
        if let Some(fixed) = policy::thai_spelling(&word, dict::thai()) {
            detection = Some(righttype::detect::Detection {
                corrected: fixed,
                confidence: righttype::detect::Confidence::High,
                evidence: righttype::detect::Evidence::ExactDictionary,
            });
        }
    }
    // A common Thai misspelling (opt-in): put right in Auto, with its own
    // message and the way back.
    let mut spelling: Option<(String, String)> = None;
    if detection.is_none()
        && !seed_run
        && mode_now == Mode::Auto
        && fixes_spelling()
        && policy::supported_layout_id(layout_id(effective_layout()))
            == Some(policy::InputLayout::ThaiKedmanee)
    {
        if let Some((fixed, wrong, right)) = righttype::spelling::fix(&word, dict::thai()) {
            if !SPELLING_KEPT.with(|k| k.borrow().iter().any(|w| w == wrong)) {
                detection = Some(righttype::detect::Detection {
                    corrected: fixed,
                    confidence: righttype::detect::Confidence::High,
                    evidence: righttype::detect::Evidence::ExactDictionary,
                });
                spelling = Some((wrong.to_string(), right.to_string()));
            }
        }
    }
    // One of the typist's own misspellings (Settings → Snippets, "My
    // typo"): put right in Auto like the built-in ones.
    if detection.is_none()
        && !seed_run
        && mode_now == Mode::Auto
        && !SPELLING_KEPT.with(|k| k.borrow().contains(&word))
    {
        let right = SNIPPETS
            .read()
            .ok()
            .and_then(|l| righttype::snippets::find_typo(&l, &word).map(str::to_string));
        if let Some(right) = right {
            spelling = Some((word.clone(), right.clone()));
            detection = Some(righttype::detect::Detection {
                corrected: right,
                confidence: righttype::detect::Confidence::High,
                evidence: righttype::detect::Evidence::ExactDictionary,
            });
        }
    }
    // An English prefix written as style has it (`relogin` → `re-login`), in
    // Auto, in prose: not in code, nor in an address bar.
    if detection.is_none()
        && !seed_run
        && mode_now == Mode::Auto
        && fixes_hyphens()
        && !crate::focus::completes_inline()
        && policy::supported_layout_id(layout_id(effective_layout()))
            == Some(policy::InputLayout::UsQwerty)
        && !SPELLING_KEPT.with(|k| k.borrow().contains(&word))
    {
        if let Some(fixed) = righttype::english::hyphenated(&word, dict::english()) {
            spelling = Some((word.clone(), fixed.clone()));
            detection = Some(righttype::detect::Detection {
                corrected: fixed,
                confidence: righttype::detect::Confidence::High,
                evidence: righttype::detect::Evidence::ExactDictionary,
            });
        }
    }

    // CapsLock left on by accident (`hELLO`, or Thai typed with every key
    // shifted). Auto puts it right and turns CapsLock off, with a way back:
    // capitals may be meant (code, acronyms), and one Shift+Backspace (or
    // Ctrl+Shift+CapsLock) restores them and CapsLock. Manual and Suggest
    // offer it as a hint that Tab takes.
    let mut caps_accident = false;
    if detection.is_none() && !seed_run && caps_on() && mode_now != Mode::Code {
        let layout_now = policy::supported_layout_id(layout_id(effective_layout()));
        if let Some(meant) = layout_now.and_then(|l| policy::caps_accident(&word, l, dict::thai()))
        {
            detection = Some(righttype::detect::Detection {
                corrected: meant,
                confidence: righttype::detect::Confidence::High,
                evidence: righttype::detect::Evidence::ExactDictionary,
            });
            caps_accident = true;
        }
    }

    // Auto mode commits only at this boundary; Manual mode retains the token for
    // Shift+Backspace.
    let mode_for_word = if caps_accident && mode_now != Mode::Auto {
        Mode::Suggest
    } else if mode_now == Mode::Code {
        if code_hint {
            Mode::Suggest
        } else {
            Mode::Auto
        }
    } else {
        mode_now
    };
    // Why, for the palette's "Why?" (the reason only, never the word).
    let why = {
        use righttype::why::{self, Why};
        if seed_run {
            Why::KeptSeed
        } else {
            match (&detection, mode_for_word) {
                (Some(_), Mode::Auto) if spelling.is_some() => Why::FixedSpelling,
                (Some(_), Mode::Auto) if caps_accident => Why::FixedCaps,
                (Some(_), Mode::Auto) if mode_now == Mode::Code => Why::FixedCode,
                (Some(d), Mode::Auto) => why::fixed(
                    &d.corrected,
                    d.evidence == righttype::detect::Evidence::ExactDictionary,
                ),
                (Some(_), Mode::Suggest) => Why::Suggested,
                (Some(_), _) => Why::KeptManual,
                (None, _) => active_layout
                    .map(|l| why::kept(&word, l, dict::english(), dict::thai()))
                    .unwrap_or(Why::KeptUnknown),
            }
        }
    };
    LAST_WHY.store(why as u8, Ordering::Relaxed);
    let swallow = match (mode_for_word, detection) {
        (Mode::Auto, Some(d)) => {
            let mut corrected = d.corrected.clone();
            let done = maybe_correct(&word, Some(vk), d);
            if done {
                let mut said =
                    righttype::i18n::trf(righttype::i18n::T::SayFixed, &[("word", &corrected)]);
                crate::overlay::announce(&said);
                said.zeroize();
            }
            if let (true, Some((wrong, right))) = (done, spelling.as_ref()) {
                STATE.with(|s| {
                    if let Some(u) = s.borrow_mut().undo.as_mut() {
                        u.kind = UndoKind::Spelling;
                    }
                });
                SPELLING_FIXED.with(|f| *f.borrow_mut() = wrong.clone());
                SPELLING_AT.with(|t| t.set(Some(Instant::now())));
                diag::note("common misspelling put right", &[]);
                crate::overlay::show_at(
                    &righttype::i18n::trf(
                        righttype::i18n::T::ToastSpellingFixed,
                        &[("wrong", wrong.as_str()), ("right", right.as_str())],
                    ),
                    crate::caret::hint_anchor(),
                );
            } else if done && !caps_accident && crate::caret::is_enabled() {
                // The first few fixes of a session show how to take one back.
                // `try_update` replaces `fetch_update` only on newer toolchains.
                #[allow(deprecated)]
                let tip_due = UNDO_TIPS_LEFT
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
                    .is_ok();
                if tip_due {
                    crate::overlay::badge_at_caret(righttype::i18n::tr(
                        righttype::i18n::T::TipShiftBackspace,
                    ));
                }
            }
            if done && caps_accident {
                inject::toggle_capslock();
                STATE.with(|s| {
                    if let Some(u) = s.borrow_mut().undo.as_mut() {
                        u.kind = UndoKind::CapsAccident;
                    }
                });
                diag::note("CapsLock on by accident: word put right, CapsLock off", &[]);
                crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastCapsOff));
            } else if done {
                if caps_on() && layout_of(&corrected) == policy::InputLayout::ThaiKedmanee {
                    inject::toggle_capslock();
                }
                // Shift+Backspace right after an automatic correction flips it
                // back — the undo gesture people reach for first.
                remember_completed(&corrected, vk, true);
            }
            corrected.zeroize();
            done
        }
        (Mode::Suggest, Some(d)) => {
            // Show *what* would be written, not just that something would:
            // a hint you cannot read is a hint you cannot judge.
            let mut hint = if caps_accident {
                diag::note("CapsLock on by accident? offered the word as meant", &[]);
                format!("Caps: {}  ·  Tab", d.corrected)
            } else {
                format!("{}  ·  Tab", d.corrected)
            };
            STATE.with(|s| {
                s.borrow_mut().suggestion = Some(SuggestionRecord {
                    caps: caps_accident,
                    original: policy::shown_with_caps(&word, caps_on()),
                    corrected: d.corrected,
                    boundary_vk: vk,
                    created: Instant::now(),
                    hwnd: GetForegroundWindow().0 as isize,
                    focus_generation: crate::focus::generation(),
                });
            });
            crate::overlay::show_at(&hint, crate::caret::hint_anchor());
            hint.zeroize();
            false
        }
        _ => false,
    };
    if !swallow {
        if converted && caps_on() {
            inject::toggle_capslock();
        }
        remember_as_shown(&word, vk, converted);
    }
    word.zeroize();
    swallow
}

/// Suggest mode, mid-word: when the keys typed so far clearly read as Thai
/// (the same bar Auto uses before it rewrites anything), show that reading
/// next to the cursor; Tab then flips the word. Nothing changes on screen
/// unless the typist asks.
fn show_live_hint() {
    let (run, guarding) = STATE.with(|s| {
        let st = s.borrow();
        (st.buf.current().to_string(), st.seed.guarding())
    });
    if guarding || run.is_empty() {
        return;
    }
    let reading = policy::live_reading(&run, false, dict::english(), dict::thai());
    if let policy::Reading::Thai(mut thai) = reading {
        let mut hint = format!("{thai}  ·  Tab");
        crate::overlay::show_at(&hint, crate::caret::hint_anchor());
        hint.zeroize();
        thai.zeroize();
        let here = LiveHint {
            hwnd: unsafe { GetForegroundWindow() }.0 as isize,
            focus_generation: crate::focus::generation(),
            created: Instant::now(),
        };
        STATE.with(|s| s.borrow_mut().live_hint = Some(here));
        LIVE_HINT_SHOWN.with(|c| c.set(true));
    } else if LIVE_HINT_SHOWN.with(|c| c.replace(false)) {
        // The reading died: take the hint (and its "Tab") off the screen.
        crate::overlay::dismiss();
    }
    let mut run = run;
    run.zeroize();
}

/// Auto, mid-word: where the keys are heading (`→ สวัสด`), next to the
/// cursor, before Auto is sure enough to rewrite anything
/// ([`policy::preview`]). Only with the cursor tags on.
fn show_preview() {
    if !crate::caret::is_enabled() {
        return;
    }
    let (run, guarding, owned) = STATE.with(|s| {
        let st = s.borrow();
        (
            st.buf.current().to_string(),
            st.seed.guarding(),
            st.owned.is_some(),
        )
    });
    let mut run = run;
    let thai = (!guarding && !owned)
        .then(|| policy::preview(&run, dict::english(), dict::thai()))
        .flatten();
    run.zeroize();
    match thai {
        Some(mut thai) => {
            // Only the system caret (Windows answers it without asking the
            // app): this runs on every key, and the UI Automation fallback
            // could wait on a slow app. No system caret, no preview.
            let Some(caret) = crate::caret::caret_rect() else {
                thai.zeroize();
                return;
            };
            let mut tag = format!("→ {thai}");
            crate::overlay::badge_at(&tag, caret);
            tag.zeroize();
            thai.zeroize();
            PREVIEW_SHOWN.with(|c| c.set(true));
        }
        None => take_down_preview(),
    }
}

fn take_down_preview() {
    if PREVIEW_SHOWN.with(|c| c.replace(false)) {
        crate::overlay::dismiss();
    }
}

thread_local! {
    /// An Auto preview is on screen.
    static PREVIEW_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A live hint is on screen (so it can be taken down when it no longer
    /// applies).
    static LIVE_HINT_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A seed phrase was just recognised: drop every copy of recently typed text
/// we still hold (Undo record, last word, pending Suggest hint), so the words
/// that came before the threshold do not outlive it in this process.
fn forget_recent_text() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.undo = None;
        st.recent.clear();
        st.suggestion = None;
        st.live_hint = None;
    });
    crate::overlay::dismiss();
}

/// Keep the word a boundary just completed (as it is on screen), for
/// Shift+Backspace right after it.
/// [`remember_completed`] for a word the app shows as typed: with CapsLock
/// on, its letters are in the other case from the keys kept for it.
unsafe fn remember_as_shown(word: &str, boundary_vk: u16, converted: bool) {
    let mut shown = policy::shown_with_caps(word, caps_on());
    remember_completed(&shown, boundary_vk, converted);
    shown.zeroize();
}

fn remember_completed(word: &str, boundary_vk: u16, converted: bool) {
    let exe = STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.recent
            .push(word, boundary_literal(boundary_vk), converted);
        st.app_exe.clone()
    });
    if let Some(exe) = exe {
        crate::habits::record_word(&exe, word);
    }
}

/// Switch the focused field to `layout` (the per-field habit, on focus).
///
/// # Safety
/// UI (hook) thread only.
pub unsafe fn switch_layout(layout: policy::InputLayout) {
    // A focus event can be delivered while the keyboard path is running on
    // this thread (it pumps messages while injecting); never re-enter it.
    if STATE.with(|s| s.try_borrow_mut().is_err()) || INJECTING.load(Ordering::Relaxed) {
        return;
    }
    activate_layout(layout);
}

/// Keys that move the caret or change text other than by typing after it.
fn moves_or_edits(vk: u16) -> bool {
    [
        VK_BACK, VK_DELETE, VK_INSERT, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_HOME,
        VK_END, VK_PRIOR, VK_NEXT,
    ]
    .iter()
    .any(|k| k.0 == vk)
}

/// Remember which English and which Thai keyboard the typist uses, from the
/// keyboard active now: text converted to the other language then follows
/// that keyboard's table (its punctuation, its letters).
fn note_keyboard(hkl: HKL) {
    if let Some(v) = policy::english_variant_of(layout_id(hkl)) {
        righttype::layout::set_english_variant(v);
    }
    if let Some(v) = policy::thai_variant_of(layout_id(hkl)) {
        righttype::layout::set_thai_variant(v);
    }
}

/// The foreground window's keyboard when RightType started.
static STARTUP_LAYOUT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Note the keyboard of the window in use, first thing at startup, before
/// any RightType window can take focus.
pub fn remember_startup_layout() {
    let hkl = unsafe { foreground_layout() };
    STARTUP_LAYOUT.store(hkl.0 as isize, std::sync::atomic::Ordering::Relaxed);
}

/// At startup: which table each installed keyboard follows, and the Thai and
/// English keyboards among the installed layouts. Only once: `install` also
/// runs on every hook reinstall (sleep, session change, hook loss), and by
/// then the keyboards actually used are known.
unsafe fn detect_keyboards() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut first = false;
    ONCE.call_once(|| first = true);
    if !first {
        return;
    }
    let facts = installed_keyboards();
    righttype::layout::registry::set_facts(facts.clone());
    // The keyboard in use when RightType started (read before any of its
    // windows, such as Welcome, could take focus) counts first; then the
    // first English keyboard installed, and of the Thai keyboards the one
    // used last (saved with the settings) rather than whichever Windows lists
    // first.
    let active = HKL(STARTUP_LAYOUT.load(std::sync::atomic::Ordering::Relaxed) as _);
    if !active.0.is_null() {
        note_keyboard(active);
    }
    let count = GetKeyboardLayoutList(None);
    if count <= 0 {
        return;
    }
    let mut list = vec![HKL::default(); count as usize];
    let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
    let installed = &list[..got];
    if policy::english_variant_of(layout_id(active)).is_none() {
        if let Some(hkl) = installed
            .iter()
            .find(|h| policy::english_variant_of(layout_id(**h)).is_some())
        {
            note_keyboard(*hkl);
        }
    }
    if policy::thai_variant_of(layout_id(active)).is_none() {
        let thai: Vec<_> = installed
            .iter()
            .filter_map(|h| policy::thai_variant_of(layout_id(*h)))
            .collect();
        if let Some(v) =
            righttype::layout::registry::prefer(&thai, righttype::layout::thai_variant())
        {
            righttype::layout::set_thai_variant(v);
        }
    }
    let handles: Vec<u32> = installed.iter().map(|h| layout_id(*h)).collect();
    THAI_UNREADABLE.store(
        righttype::layout::registry::thai_unreadable(&handles, &facts),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Thai keyboards are installed but none is one RightType can read: nothing
/// is converted into Thai, since the table would be a guess.
static THAI_UNREADABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The Thai and English keyboards installed, by the high word of their
/// handle (`0xF000` + the registry's "Layout Id", or the keyboard's own
/// identifier), with the table each follows, told apart by the layout file
/// (see [`righttype::layout::registry::classify_layout_file`]). A keyboard
/// whose file cannot be read or is not known is left out: it is not guessed.
fn installed_keyboards() -> righttype::layout::registry::KeyboardFacts {
    use righttype::layout::registry::{classify_layout_file, KeyboardFacts, Language};
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    fn read(key: &str, value: &str) -> Option<String> {
        let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut buf = [0u16; 128];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    // Windows' own Thai keyboards, and every other keyboard installed (a
    // keyboard installed from a file, such as Manoonchai, gets an
    // identifier like `A000041E`).
    let mut klids: Vec<String> = ["0001041E", "0002041E", "0003041E"]
        .iter()
        .map(|k| k.to_string())
        .collect();
    for k in installed_keyboard_ids() {
        if !klids.iter().any(|o| o.eq_ignore_ascii_case(&k)) {
            klids.push(k);
        }
    }
    let mut facts = KeyboardFacts::default();
    for klid in klids {
        if klid.len() != 8 {
            continue;
        }
        let language = match u32::from_str_radix(&klid[4..], 16).map(|l| l & 0x3FF) {
            Ok(0x1E) => Language::Thai,
            Ok(0x09) => Language::English,
            _ => continue,
        };
        let key = format!("SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\\{klid}");
        let Some(file) = read(&key, "Layout File") else {
            continue;
        };
        let text = read(&key, "Layout Text").unwrap_or_default();
        let Some(table) = classify_layout_file(language, &file, &text) else {
            continue;
        };
        let list = match language {
            Language::Thai => &mut facts.thai,
            Language::English => &mut facts.english,
        };
        if let Some(id) =
            read(&key, "Layout Id").and_then(|id| u16::from_str_radix(id.trim(), 16).ok())
        {
            list.push((0xF000 | id, table));
        }
        // The keyboard's own identifier, as some handles carry it. Zero is
        // the language's default keyboard, which needs no entry.
        if let Ok(high) = u16::from_str_radix(&klid[..4], 16) {
            if high != 0 {
                list.push((high, table));
            }
        }
    }
    facts
}

/// The keyboard identifiers (`0000041E`, `A000041E`, …) Windows lists under
/// `Keyboard Layouts`.
fn installed_keyboard_ids() -> Vec<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    let path: Vec<u16> = "SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\0"
        .encode_utf16()
        .collect();
    let mut key = HKEY::default();
    let mut out = Vec::new();
    unsafe {
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            0,
            KEY_READ,
            &mut key,
        )
        .is_err()
        {
            return out;
        }
        for i in 0..2048u32 {
            let mut name = [0u16; 64];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(
                key,
                i,
                windows::core::PWSTR(name.as_mut_ptr()),
                &mut len,
                None,
                windows::core::PWSTR::null(),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            out.push(String::from_utf16_lossy(&name[..len as usize]));
        }
        let _ = RegCloseKey(key);
    }
    out
}

/// The whole 32-bit handle: [`policy::supported_layout_id`] reads both the
/// language and the keyboard from it, since sharing a language does not make
/// two keyboards' physical-key mappings compatible.
fn layout_id(hkl: HKL) -> u32 {
    hkl.0 as usize as u32
}

/// Switch the foreground window to one exact layout supported by the v1 mapping
/// tables. No-op if that layout is not installed.
unsafe fn activate_layout(target: policy::InputLayout) {
    // Already there, or already on its way there.
    if policy::supported_layout_id(layout_id(effective_layout())) == Some(target) {
        return;
    }
    let count = GetKeyboardLayoutList(None);
    if count <= 0 {
        return;
    }
    let mut list = vec![HKL::default(); count as usize];
    let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
    list.truncate(got);
    // With several keyboards for the language (US and UK, Kedmanee and
    // Pattachote), the one whose table is in use comes first.
    list.sort_by_key(|h| !policy::is_preferred_layout(layout_id(*h)));
    for hkl in list.iter() {
        if policy::supported_layout_id(layout_id(*hkl)) == Some(target) {
            request_hkl(*hkl);
            return;
        }
    }
}

/// Ask the foreground window to switch to exactly this keyboard. Returns
/// whether a switch was asked for (not when it is already there or on its way,
/// or the keyboard is not one RightType reads).
unsafe fn request_hkl(hkl: HKL) -> bool {
    let Some(layout) = policy::supported_layout_id(layout_id(hkl)) else {
        return false;
    };
    if effective_layout().0 == hkl.0 {
        return false;
    }
    let foreground = GetForegroundWindow();
    let mut gui = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let thread_id = GetWindowThreadProcessId(foreground, None);
    let target = if GetGUIThreadInfo(thread_id, &mut gui).is_ok() && !gui.hwndFocus.0.is_null() {
        gui.hwndFocus
    } else {
        foreground
    };
    let _ = PostMessageW(
        target,
        WM_INPUTLANGCHANGEREQUEST,
        WPARAM(0),
        LPARAM(hkl.0 as isize),
    );
    // Some modern apps put focus on a custom child that does not pass
    // the request to DefWindowProc. Also notify the top-level window;
    // requesting the same explicit HKL twice is idempotent.
    if target != foreground {
        let _ = PostMessageW(
            foreground,
            WM_INPUTLANGCHANGEREQUEST,
            WPARAM(0),
            LPARAM(hkl.0 as isize),
        );
    }
    let root = GetAncestor(foreground, GA_ROOT);
    if !root.0.is_null() && root != target && root != foreground {
        let _ = PostMessageW(
            root,
            WM_INPUTLANGCHANGEREQUEST,
            WPARAM(0),
            LPARAM(hkl.0 as isize),
        );
    }
    // Record that we are waiting for this HKL to activate, and adopt it
    // as the context's layout now: this switch is ours, so it must not
    // read as the context change that drops the token in progress.
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.pending_hkl = Some(PendingLayout {
            hkl: hkl.0 as isize,
            since: Instant::now(),
        });
        st.last_hkl = hkl.0 as isize;
    });
    // No toast here: a layout switch happens on every ignition, which is
    // too frequent for a message. Just a small TH/EN tag at the caret,
    // where the eyes are (shown after this callback returns).
    crate::caret::layout_switched(layout);
    true
}

/// Accept the one context-bound, non-destructive suggestion produced at the
/// previous boundary. Any intervening non-modifier key or context change clears
/// the record before this function can run.
unsafe fn accept_suggestion() {
    let Some(mut suggestion) = STATE.with(|s| s.borrow_mut().suggestion.take()) else {
        return;
    };
    let backspaces = suggestion.original.chars().count() + 1;
    if !inject::apply(
        backspaces,
        &suggestion.corrected,
        Some(suggestion.boundary_vk),
    ) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrSuggestInject));
        return;
    }

    let mut restore = format!(
        "{}{}",
        suggestion.original,
        boundary_literal(suggestion.boundary_vk)
    );
    set_undo(
        suggestion.corrected.chars().count() + 1,
        &restore,
        UndoKind::Manual,
    );
    restore.zeroize();
    if suggestion.caps && caps_on() {
        inject::toggle_capslock();
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastCapsOff));
    }
    crate::stats::record_manual();
    habit_correction(
        has_thai(&suggestion.original),
        has_thai(&suggestion.corrected),
    );

    activate_layout(layout_of(&suggestion.corrected));
    suggestion.original.zeroize();
    suggestion.corrected.zeroize();
}

/// A counted word in this app changed language (see `habits`).
fn habit_correction(was_thai: bool, now_thai: bool) {
    if let Some(exe) = STATE.with(|s| s.borrow().app_exe.clone()) {
        crate::habits::correct_word(&exe, was_thai, now_thai);
    }
}

fn has_thai(text: &str) -> bool {
    text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// Shift+Backspace with no word in progress: flip one more of the recent
/// words back to the other layout (see [`Recent`]). The whole span from that
/// word to the caret is retyped in one injection, and it is one Undo step.
unsafe fn flip_back_recent() {
    // Right after a CapsLock fix, Shift+Backspace is its way back (flipping
    // `Hello` to Thai would be no use).
    if STATE.with(|s| {
        s.borrow().undo.as_ref().is_some_and(|u| {
            matches!(
                u.kind,
                UndoKind::CapsAccident | UndoKind::Spelling | UndoKind::Snippet
            )
        })
    }) {
        undo_last_correction();
        return;
    }
    let conv = shown_converter();
    let Some(step) = STATE.with(|s| s.borrow().recent.next_step(conv)) else {
        e2e_trace("flip back: no recent word".to_string());
        diag::note("Shift+Backspace: nothing to flip", &[]);
        // Say so: a press that does nothing looks like one that failed.
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastNothingToFlip));
        return;
    };
    if step.insert.chars().count() + 1 == step.backspaces
        && step.restore.strip_suffix(step.boundary) == Some(step.insert.as_str())
    {
        // Nothing changes on screen (a number, say): just move on to the
        // word before it on the next press.
        let conv = shown_converter();
        STATE.with(|s| s.borrow_mut().recent.commit(conv));
        return;
    }
    // The deleted characters, so a text box that has not caught up with
    // the last keys is not edited (CI, Windows 11 Notepad: the flip read
    // `l;ylfu ` before `8iy[ ` reached the box).
    inject::expect_before_caret(&step.restore);
    if !inject::apply(
        step.backspaces,
        &step.insert,
        Some(boundary_vk(step.boundary)),
    ) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().recent.clear());
        return;
    }
    let conv = shown_converter();
    STATE.with(|s| s.borrow_mut().recent.commit(conv));
    set_undo(
        step.insert.chars().count() + 1,
        &step.restore,
        UndoKind::Manual,
    );
    crate::stats::record_manual();
    // Flipping back a word RightType converted by itself is the clearest
    // "that was a real word" there is.
    if let Some(word) = step.learn.as_deref() {
        crate::learn::learn_now(word);
        note_rejection();
    }
    habit_correction(step.was_thai, step.now_thai);
    activate_layout(layout_of(&step.newest));
    // Every press says how far back it reached, so the next press is never
    // a guess.
    let more = STATE.with(|s| s.borrow().recent.len()) > step.words;
    if step.reverts {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
    } else if step.words > 1 {
        crate::overlay::show(&righttype::i18n::trf(
            righttype::i18n::T::ToastFlippedWords,
            &[("n", &step.words.to_string())],
        ));
    } else {
        crate::overlay::show(righttype::i18n::tr(if more {
            righttype::i18n::T::ToastFlippedOneMore
        } else {
            righttype::i18n::T::ToastFlippedOneBack
        }));
    }
}

/// How long Shift+Backspace is held before the rest of the run is flipped.
const FLIP_HOLD: Duration = Duration::from_millis(400);

thread_local! {
    /// When Shift+Backspace went down, and whether holding it has acted.
    static FLIP_DOWN: std::cell::Cell<Option<(Instant, bool)>> =
        const { std::cell::Cell::new(None) };
}

/// A repeat of a held Shift+Backspace: once it has been held for
/// FLIP_HOLD, flip the rest of the run (once); otherwise nothing.
unsafe fn flip_held_repeat() {
    let held = FLIP_DOWN.with(|f| {
        f.get()
            .filter(|(at, done)| !done && at.elapsed() >= FLIP_HOLD)
            .is_some()
    });
    if held {
        FLIP_DOWN.with(|f| f.set(f.get().map(|(at, _)| (at, true))));
        flip_rest_of_run();
    }
}

/// Shift+Backspace held: flip every word of the run the presses have not
/// reached yet, in one step. Undo (or one more press) puts them back.
unsafe fn flip_rest_of_run() {
    if !STATE.with(|s| s.borrow().buf.current().is_empty()) {
        return;
    }
    let conv = shown_converter();
    let Some(step) = STATE.with(|s| s.borrow().recent.rest_step(conv)) else {
        return;
    };
    e2e_trace(format!("flip: held, {} words", step.words));
    // The deleted characters, so a text box that has not caught up with
    // the last keys is not edited (CI, Windows 11 Notepad: the flip read
    // `l;ylfu ` before `8iy[ ` reached the box).
    inject::expect_before_caret(&step.restore);
    if !inject::apply(
        step.backspaces,
        &step.insert,
        Some(boundary_vk(step.boundary)),
    ) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().recent.clear());
        return;
    }
    let conv = shown_converter();
    STATE.with(|s| s.borrow_mut().recent.commit_rest(conv));
    set_undo(
        step.insert.chars().count() + 1,
        &step.restore,
        UndoKind::Manual,
    );
    crate::stats::record_manual();
    habit_correction(step.was_thai, step.now_thai);
    activate_layout(layout_of(&step.newest));
    crate::overlay::show(&righttype::i18n::trf(
        righttype::i18n::T::ToastFlippedWords,
        &[("n", &step.words.to_string())],
    ));
}

/// The recent words (oldest first, as on screen) and where each is before
/// the caret, for the palette's list. The caller wipes the words.
pub fn recent_words() -> (Vec<String>, Vec<(usize, usize)>) {
    STATE.with(|s| {
        let st = s.borrow();
        (st.recent.words(), st.recent.spans())
    })
}

/// A word as it would be after a flip (the palette shows it).
pub fn flipped(word: &str) -> String {
    shown_converter()(word)
}

/// The palette's "flip these": flip the recent words at `picked` (oldest
/// first) and leave the others, in the window they were typed in.
///
/// # Safety
/// UI (hook) thread.
pub unsafe fn flip_picked(picked: &[usize]) {
    use righttype::i18n::{tr, trf, T};
    let same_window = STATE.with(|s| s.borrow().last_hwnd) == GetForegroundWindow().0 as isize;
    let conv = shown_converter();
    let step = STATE.with(|s| s.borrow().recent.flip_picked(picked, conv));
    let Some(step) = step.filter(|_| same_window) else {
        crate::overlay::show(tr(T::ToastNothingToFlip));
        return;
    };
    // The deleted characters, so a text box that has not caught up with
    // the last keys is not edited (CI, Windows 11 Notepad: the flip read
    // `l;ylfu ` before `8iy[ ` reached the box).
    inject::expect_before_caret(&step.restore);
    if !inject::apply(
        step.backspaces,
        &step.insert,
        Some(boundary_vk(step.boundary)),
    ) {
        crate::overlay::show(tr(T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().recent.clear());
        return;
    }
    // The words on screen are no longer the ones recorded: start afresh.
    STATE.with(|s| s.borrow_mut().recent.clear());
    set_undo(
        step.insert.chars().count() + 1,
        &step.restore,
        UndoKind::Manual,
    );
    crate::stats::record_manual();
    if let Some(word) = step.learn.as_deref() {
        crate::learn::learn_now(word);
        note_rejection();
    }
    activate_layout(layout_of(&step.newest));
    diag::note(
        "palette: picked recent words flipped",
        &[("words", step.words.into())],
    );
    crate::overlay::show(&trf(
        T::ToastFlippedWords,
        &[("n", &step.words.to_string())],
    ));
}

/// The layout the focused window is typing in, as the conversion layer names it.
unsafe fn current_layout_id() -> Option<LayoutId> {
    policy::flip_layout_id(layout_id(effective_layout()))
}

/// Flip a word as the app shows it. With CapsLock on, English on screen is in
/// the other case from its keys (`L;YLFU` is สวัสดี), and English put back
/// is shown that way too. CapsLock clears the recent words, so its state now
/// is the one they were typed with.
fn convert_shown(word: &str, layout: Option<LayoutId>) -> String {
    let caps = unsafe { caps_on() };
    let keys = policy::shown_with_caps(word, caps);
    policy::shown_with_caps(&auto_convert_with_layout(&keys, layout), caps)
}

/// [`convert_shown`] for the layout in use now. Take it *before* borrowing
/// the recent words: finding the layout reads `STATE`, which they are part
/// of, so asking from inside their callback panics ("already mutably
/// borrowed"), as the e2e run showed.
fn shown_converter() -> impl Fn(&str) -> String + Copy {
    let layout = unsafe { current_layout_id() };
    move |word| convert_shown(word, layout)
}

/// Manual: flip the layout of the word currently in the buffer, in place. A no-op
/// when the buffer is empty (e.g. an auto-repeat after the word was already
/// converted) — the caller swallows the key either way.
unsafe fn convert_last_word() {
    let mut word = STATE.with(|s| s.borrow().buf.current().to_string());
    if word.is_empty() {
        // Right after a boundary: flip the word before it — and, pressed
        // again, the word before that one too (up to `Recent::CAP`).
        flip_back_recent();
        return;
    }

    let backspaces = word.chars().count();
    // The buffer keeps the keys as if CapsLock were off; the app shows them
    // with it.
    let layout = current_layout_id();
    let mut converted = auto_convert_with_layout(&word, layout);
    let changed = converted != word;
    if changed {
        let mut shown = policy::shown_with_caps(&converted, caps_on());
        inject::expect_before_caret(&policy::shown_with_caps(&word, caps_on()));
        let injected = inject::apply(backspaces, &shown, None);
        shown.zeroize();
        if !injected {
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
            word.zeroize();
            converted.zeroize();
            return;
        }
        // The token stays open: the buffer now describes the flipped text, so
        // the typist can carry on and the boundary still sees the whole word.
        // It is theirs now — Auto will not reinterpret it.
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            let learn = st.mark == TokenMark::Converted;
            st.buf.replace(&converted);
            st.mark = TokenMark::Decided { learn };
        });
        let mut shown = policy::shown_with_caps(&word, caps_on());
        set_undo(converted.chars().count(), &shown, UndoKind::Manual);
        shown.zeroize();
        crate::stats::record_manual();
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastFlippedOne));

        // Switch language layout to the one of the converted word
        activate_layout(layout_of(&converted));
    }
    word.zeroize();
    converted.zeroize();
}

/// Put the raw keystrokes back and stop owning the run. Returns `true` if we
/// were owning anything (and therefore handled the key).
unsafe fn withdraw_owned_run() -> bool {
    let run = STATE.with(|s| s.borrow().buf.current().to_string());
    withdraw_owned_run_to(&run)
}

/// Put `run` (the keys as typed) back in place of the run we own.
unsafe fn withdraw_owned_run_to(run: &str) -> bool {
    let Some(owned) = STATE.with(|s| s.borrow_mut().owned.take()) else {
        return false;
    };
    let shown = policy::shown_with_caps(run, caps_on());
    let delta = render::delta(&owned.rendered, &shown);
    inject::expect_before_caret(&owned.rendered);
    if !delta.is_empty() && !inject::apply(delta.backspaces, &delta.insert, None) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
    }
    true
}

/// A run we own has reached a word boundary: keep what is on screen, record it
/// for Undo, and let the Thai layout carry the rest of the sentence. Returns
/// the text now on screen for the run.
///
/// The boundary key itself passes through to the app after this, so the Undo
/// record covers it too — otherwise Undo would delete the boundary plus the
/// last rendered character and leave the first one behind.
unsafe fn anchor_owned_run(run: &str, boundary_vk: u16) -> Option<String> {
    let owned = STATE.with(|s| s.borrow_mut().owned.take())?;
    let mut restore = format!(
        "{}{}",
        policy::shown_with_caps(run, caps_on()),
        boundary_literal(boundary_vk)
    );
    set_undo(
        owned.rendered.chars().count() + 1,
        &restore,
        UndoKind::AutoWord,
    );
    restore.zeroize();
    if caps_on() {
        inject::toggle_capslock();
    }
    crate::stats::record_auto();
    activate_layout(policy::InputLayout::ThaiKedmanee);
    Some(owned.rendered.clone())
}

/// The layout a piece of text ends in, for switching to after a correction or flip.
fn layout_of(text: &str) -> policy::InputLayout {
    let trailing_thai = text
        .chars()
        .rev()
        .find(|c| !c.is_ascii_punctuation())
        .is_some_and(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    if trailing_thai {
        policy::InputLayout::ThaiKedmanee
    } else {
        policy::InputLayout::UsQwerty
    }
}

/// Move the screen to the current run's best reading, taking ownership of the
/// run's characters if the reading is not simply "as typed".
///
/// Returns `true` when the triggering key was consumed (we rendered the run
/// ourselves and the key must not also reach the app).
unsafe fn reconcile_run() -> bool {
    reconcile_run_with(|backspaces, text, trailing_vk| inject::apply(backspaces, text, trailing_vk))
}

/// [`reconcile_run`] with the injector supplied, so the partial-failure seam
/// can be exercised without Win32.
unsafe fn reconcile_run_with<F>(apply: F) -> bool
where
    F: FnOnce(usize, &str, Option<u16>) -> bool,
{
    let (run, poisoned, holding) = STATE.with(|s| {
        let st = s.borrow();
        (
            st.buf.current().to_string(),
            st.buf.is_poisoned(),
            st.owned.is_some(),
        )
    });

    // An over-long token is dropped unanalysed; we cannot describe the screen
    // any more, so let go of it rather than editing text we cannot account for.
    if poisoned {
        if holding {
            STATE.with(|s| s.borrow_mut().owned = None);
        }
        return false;
    }
    if !holding && run.is_empty() {
        return false;
    }

    let mut reading = policy::live_reading(&run, holding, dict::english(), dict::thai());

    // The seed-phrase stream guard outranks any reading. Mid-word this only
    // asks whether a seed phrase could be in progress; the completed token is
    // counted once, at its boundary.
    if matches!(reading, policy::Reading::Thai(_)) && STATE.with(|s| s.borrow().seed.guarding()) {
        reading = policy::Reading::AsTyped;
    }
    // A snippet's trigger being typed stays as typed, to be found at its
    // boundary (`;today` spells Thai keys).
    if matches!(reading, policy::Reading::Thai(_)) && !holding {
        let starts = policy::supported_layout_id(layout_id(effective_layout())).is_some_and(|l| {
            SNIPPETS
                .read()
                .is_ok_and(|list| righttype::snippets::starts_a_trigger(&list, &run, l))
        });
        if starts {
            reading = policy::Reading::AsTyped;
        }
    }
    e2e_trace(format!(
        "reconcile run={run:?} holding={holding} -> {reading:?}"
    ));
    match (&reading, holding) {
        (policy::Reading::Thai(_), false) => diag::note(
            "mid-word: shown as Thai",
            &[("typed", Shape::of(&run).into())],
        ),
        (policy::Reading::AsTyped, true) => diag::note(
            "mid-word: back to the keys",
            &[("typed", Shape::of(&run).into())],
        ),
        _ => {}
    }

    let target = match &reading {
        policy::Reading::AsTyped => policy::shown_with_caps(&run, caps_on()),
        policy::Reading::Thai(thai) => thai.clone(),
    };

    // What the app is showing right now. Before we own the run the current key
    // has not reached the app yet, so the screen holds the run minus that key.
    let on_screen = if holding {
        STATE.with(|s| {
            s.borrow()
                .owned
                .as_ref()
                .map(|o| o.rendered.clone())
                .unwrap_or_default()
        })
    } else {
        if matches!(reading, policy::Reading::AsTyped) {
            // Nothing to do and nothing to own: let the key through untouched.
            return false;
        }
        run.chars()
            .take(run.chars().count().saturating_sub(1))
            .collect()
    };

    let delta = render::delta(&on_screen, &target);
    // Before we own the run the app shows the keys as CapsLock shows them.
    inject::expect_before_caret(&if holding {
        on_screen.clone()
    } else {
        policy::shown_with_caps(&on_screen, caps_on())
    });
    if !delta.is_empty() && !apply(delta.backspaces, &delta.insert, None) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().owned = None);
        return false;
    }

    match reading {
        policy::Reading::AsTyped => {
            // The reading was withdrawn: the screen again equals the keystrokes,
            // so the app owns the run once more.
            STATE.with(|s| s.borrow_mut().owned = None);
        }
        policy::Reading::Thai(_) => {
            let anchor = STATE.with(|s| {
                let mut st = s.borrow_mut();
                let stable = st.owned.as_ref().map(|o| o.stable).unwrap_or(0) + 1;
                st.owned = Some(OwnedRun {
                    rendered: target.clone(),
                    stable,
                });
                stable >= policy::COMMIT_HORIZON
            });
            if anchor {
                // Stable long enough to stop second-guessing: hand the rest of
                // the sentence to the Thai layout and release the run.
                //
                // D-009: release the *run*, not the *token*. The buffer is
                // rewritten to what is now on screen, and the native Thai
                // keystrokes that follow extend it, so the boundary still sees
                // the whole word. Clearing it here is what used to leave
                // `กรดดำrent`: the boundary judged only the tail.
                let mut shown = policy::shown_with_caps(&run, caps_on());
                set_undo(target.chars().count(), &shown, UndoKind::AutoMidToken);
                shown.zeroize();
                crate::stats::record_auto();
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.owned = None;
                    st.buf.replace(&target);
                    st.mark = TokenMark::Converted;
                });
                activate_layout(policy::InputLayout::ThaiKedmanee);
            }
        }
    }
    true
}

/// Inject a completed `word` after the caller's stream and detection guards pass.
/// `boundary_vk` is the separator that completed the word, re-emitted after the
/// correction. Since D-008 the in-flight path renders through
/// [`reconcile_run`] instead, so this is the boundary path only. Returns `true`
/// if a correction was injected (caller swallows the triggering key).
unsafe fn maybe_correct(
    word: &str,
    boundary_vk: Option<u16>,
    d: righttype::detect::Detection,
) -> bool {
    maybe_correct_with(word, boundary_vk, d, |backspaces, text, trailing_vk| {
        inject::apply(backspaces, text, trailing_vk)
    })
}

unsafe fn maybe_correct_with<F>(
    word: &str,
    boundary_vk: Option<u16>,
    d: righttype::detect::Detection,
    apply: F,
) -> bool
where
    F: FnOnce(usize, &str, Option<u16>) -> bool,
{
    // Every token character reached the app (the separator itself was
    // swallowed), so delete exactly `word.len()`. The `None` case is kept for
    // callers that swallowed the triggering character before it landed.
    let backspaces = word.chars().count() - usize::from(boundary_vk.is_none());
    let mut corrected = d.corrected;
    if boundary_vk.is_some() {
        inject::expect_before_caret(&policy::shown_with_caps(word, caps_on()));
    }
    if !apply(backspaces, &corrected, boundary_vk) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        corrected.zeroize();
        return false;
    }

    // Undo target: retype the original word plus the boundary it would have
    // gotten anyway (the boundary keystroke itself never reached the app).
    // With CapsLock on the app showed the keys in the other case.
    let mut shown = policy::shown_with_caps(word, caps_on());
    let mut restore = match boundary_vk {
        Some(vk) => format!("{shown}{}", boundary_literal(vk)),
        None => shown.clone(),
    };
    shown.zeroize();
    set_undo(
        corrected.chars().count() + usize::from(boundary_vk.is_some()),
        &restore,
        UndoKind::AutoWord,
    );
    crate::stats::record_auto();
    restore.zeroize();

    // Switch to whichever language the word ends in, so the rest of the sentence
    // types natively — same idea as the live Thai path, now for the EN direction.
    let next_layout = layout_of(&corrected);
    corrected.zeroize();
    activate_layout(next_layout);
    true
}

/// Translate a raw key into a [`Key`] for the buffer, or `None` to ignore it.
unsafe fn classify(vk: u16, scan: u16) -> Option<Key> {
    // Any Ctrl/Alt chord is a command, not text: drop the in-progress word.
    if is_down(VK_CONTROL) || is_down(VK_MENU) {
        return Some(Key::Reset);
    }
    if vk == VK_BACK.0 {
        return Some(Key::Backspace);
    }
    #[cfg(debug_assertions)]
    if vk == VK_PACKET.0 && debug_e2e_accepts_injected() {
        return char::from_u32(scan as u32).map(Key::Char);
    }
    if vk == VK_SPACE.0 || vk == VK_RETURN.0 || vk == VK_TAB.0 {
        return Some(Key::Boundary);
    }
    // Navigation / editing keys move the caret: the buffered word is no longer
    // contiguous with what we'd correct, so discard it.
    if matches!(
        vk,
        v if v == VK_ESCAPE.0
            || v == VK_LEFT.0
            || v == VK_RIGHT.0
            || v == VK_UP.0
            || v == VK_DOWN.0
            || v == VK_HOME.0
            || v == VK_END.0
            || v == VK_PRIOR.0
            || v == VK_NEXT.0
            || v == VK_DELETE.0
            || v == VK_INSERT.0
    ) {
        return Some(Key::Reset);
    }
    translate(vk, scan).map(Key::Char)
}

/// The character the keystroke means, using the foreground layout and the
/// live Shift state — as if CapsLock were off. With CapsLock left on the
/// English layout shows `L;YLFU`, but the keys are the ones for สวัสดี; text
/// put back "as typed" is shown with CapsLock again
/// ([`policy::shown_with_caps`]).
unsafe fn translate(vk: u16, scan: u16) -> Option<char> {
    let mut state = [0u8; 256];
    if is_down(VK_SHIFT) {
        state[VK_SHIFT.0 as usize] = 0x80;
    }

    let hkl = effective_layout();
    let mut out = [0u16; 8];
    let n = ToUnicodeEx(vk as u32, scan as u32, &state, &mut out, 0, hkl);
    if n == 1 {
        char::from_u32(out[0] as u32).filter(|c| !c.is_control())
    } else {
        // 0 = no mapping (e.g. F-keys / modifiers), -1 = dead key, >1 = ligature.
        None
    }
}

/// Drop the buffered word if the focused window or keyboard layout changed since
/// the previous key — the buffer only describes one editing context at a time.
unsafe fn sync_context() {
    let hwnd = GetForegroundWindow();
    let tid = GetWindowThreadProcessId(hwnd, None);
    let hwnd_i = hwnd.0 as isize;

    // A layout-switch request is asynchronous. Never wait for it inside the
    // global low-level hook: a slow target window used to add up to 50 ms of
    // latency to the next physical key. While our own request is in flight the
    // context is already on the requested layout (see `effective_layout`);
    // once it lands it is confirmed, and if the app ignores it past the grace
    // period we resync `last_hkl` to the real layout without treating it as a
    // manual user layout switch that would wipe the current token buffer.
    let actual = GetKeyboardLayout(tid).0 as isize;
    let hkl_i = STATE.with(|s| {
        let mut st = s.borrow_mut();
        match st.pending_hkl {
            Some(p) if p.hkl != actual && p.since.elapsed() < PENDING_LAYOUT_GRACE => p.hkl,
            Some(p) => {
                st.pending_hkl = None;
                if p.hkl != actual && st.last_hkl == p.hkl {
                    st.last_hkl = actual;
                }
                actual
            }
            None => actual,
        }
    });

    let focus_generation = crate::focus::generation();
    let mut undo_switch = None;
    let window_changed = STATE.with(|s| {
        let mut st = s.borrow_mut();
        let changed = st.last_hwnd != hwnd_i;
        let lang_changed = st.last_hkl != hkl_i;
        // A switch the typist did not mean: it came with a shortcut
        // (Ctrl/Alt + Shift + a key), not on its own, and was not ours.
        if lang_changed && st.pending_hkl.is_none() && st.last_hwnd == hwnd_i && guards_switch() {
            let came_with = SHORTCUT.with(|c| c.get()).filter(|(at, before)| {
                at.elapsed() < SHORTCUT_SWITCH_WINDOW && *before == st.last_hkl
            });
            if let Some((_, before)) = came_with {
                undo_switch = Some(HKL(before as *mut _));
            }
        }
        let mut focus_changed = st.last_focus_generation != focus_generation;
        // A slow app can answer the focus question long after the move (CI:
        // 15 s). When the context was already reset after the move happened
        // (the window or keyboard changed since), the keys typed since are
        // in the new field: dropping them then stranded the first letter
        // (`lวัสดี`).
        if focus_changed
            && !changed
            && !lang_changed
            && crate::focus::moved_at().is_some_and(|at| at < st.context_since)
        {
            e2e_trace("focus answer came late: the run already started after the move".into());
            st.last_focus_generation = focus_generation;
            focus_changed = false;
        }
        if changed || lang_changed || focus_changed {
            e2e_trace(format!(
                "context changed: window={changed} layout={lang_changed} ({:X} -> {hkl_i:X}) focus={focus_changed}",
                st.last_hkl
            ));
            // A layout switch alone must NOT drop the pending Undo: it is
            // usually our own correction switching the layout, and the very
            // next keypress (e.g. the Undo hotkey itself) would otherwise
            // erase the record it is about to use. Window/focus changes are
            // genuine context loss.
            diag::note(
                "context changed",
                &[
                    ("window", changed.into()),
                    ("layout", lang_changed.into()),
                    ("layout_id", ((hkl_i as u64 & 0xFFFF) as i64).into()),
                    ("field", focus_changed.into()),
                ],
            );
            if changed || focus_changed {
                st.undo = None;
            }
            st.buf.clear();
            st.owned = None;
            st.mark = TokenMark::Plain;
            st.seed.reset();
            st.recent.clear();
            st.suggestion = None;
            st.live_hint = None;
            ghost_tail_forget();
            st.last_hwnd = hwnd_i;
            st.last_hkl = hkl_i;
            st.last_focus_generation = focus_generation;
            st.context_since = Instant::now();
        }
        changed
    });
    if let Some(before) = undo_switch {
        // The exact keyboard that was left, not just its language: Kedmanee
        // to Pattachote, or US to Dvorak, is a switch within one language.
        if request_hkl(before) {
            trace_note("language switch with a shortcut: undone");
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastSwitchUndone));
        }
    }
    // Re-evaluate the (heavier) app blacklist only when the window changed.
    if window_changed {
        let exe = safety::foreground_exe(hwnd);
        // Unknown process identity is not evidence that a context is safe.
        let blacklisted = exe.as_deref().map_or(true, safety::is_blacklisted_name);
        if let Some(exe) = exe.as_deref() {
            crate::apps::note_typing_in(exe);
        }
        diag::note_app(
            exe.as_deref().unwrap_or("?"),
            crate::habits::focused_class().as_deref().unwrap_or("?"),
        );
        if blacklisted {
            diag::note("app is protected: RightType stays out", &[]);
        }
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.sensitive_app = blacklisted;
            st.app_exe = exe;
        });
    }
}

/// The layout the focused app is typing in *now*: our own switch request while
/// it is in flight, otherwise what Windows reports.
///
/// The target app handles the posted `WM_INPUTLANGCHANGEREQUEST` before the
/// next keystroke's input message (posted messages are retrieved first), so a
/// key pressed right after an anchor already produces Thai there, while
/// `GetKeyboardLayout` can still say English for a moment. Translating with
/// the stale answer put Latin letters in a buffer describing Thai text.
unsafe fn effective_layout() -> HKL {
    let actual = foreground_layout();
    match STATE.with(|s| s.borrow().pending_hkl) {
        Some(p) if p.hkl != actual.0 as isize && p.since.elapsed() < PENDING_LAYOUT_GRACE => {
            HKL(p.hkl as _)
        }
        _ => actual,
    }
}

/// The keyboard layout (HKL) of whatever window currently has focus.
unsafe fn foreground_layout() -> HKL {
    let hwnd = GetForegroundWindow();
    let tid = GetWindowThreadProcessId(hwnd, None);
    GetKeyboardLayout(tid)
}

#[cfg(test)]
mod tests {
    use super::{shown_converter, Mode, OwnedRun, STATE};
    use righttype::detect::{Confidence, Detection, Evidence};

    /// The recent words call the converter while `STATE` is borrowed for
    /// writing. It must not read `STATE` itself: it did, and the app died with
    /// "already mutably borrowed" (the e2e run on PR #26).
    #[test]
    fn the_flip_converter_does_not_touch_state_while_it_is_borrowed() {
        let convert = shown_converter();
        STATE.with(|s| {
            let _held = s.borrow_mut();
            let _ = convert("l;ylfu");
            let _ = convert(".");
        });
    }

    #[test]
    fn mode_cycle_is_manual_auto_suggest() {
        assert_eq!(Mode::Manual.next(), Mode::Auto);
        assert_eq!(Mode::Auto.next(), Mode::Suggest);
        assert_eq!(Mode::Suggest.next(), Mode::Manual);
    }

    /// A failed injection while we own a run must let go of it. Keeping
    /// ownership would leave the next keystroke diffing against text we were
    /// never able to put on screen, and every edit after that would be
    /// computed from a screen state that does not exist.
    #[test]
    fn failed_reconcile_injection_releases_ownership() {
        use righttype::buffer::Key;

        STATE.with(|state| {
            let mut st = state.borrow_mut();
            st.buf.clear();
            for c in "l;ylfu".chars() {
                st.buf.observe(Key::Char(c));
            }
            // Own the run, but with nothing yet rendered, so the reconciler has
            // a non-empty delta and must call the injector.
            st.owned = Some(OwnedRun {
                rendered: String::new(),
                stable: 1,
            });
        });
        let stats_before = crate::stats::snapshot();

        let consumed = unsafe { super::reconcile_run_with(|_backspaces, _text, _vk| false) };

        assert!(!consumed, "a failed injection must not swallow the key");
        STATE.with(|state| {
            assert!(
                state.borrow().owned.is_none(),
                "ownership must be released when the screen could not be updated"
            );
        });
        assert_eq!(crate::stats::snapshot(), stats_before);
        STATE.with(|state| state.borrow_mut().buf.clear());
    }

    #[test]
    fn failed_auto_injection_has_no_success_side_effects() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.undo = None;
            state.pending_hkl = None;
            state.owned = None;
        });
        let stats_before = crate::stats::snapshot();
        let detection = Detection {
            corrected: "สวัสดี".to_string(),
            confidence: Confidence::High,
            evidence: Evidence::ExactDictionary,
        };

        let committed = unsafe {
            super::maybe_correct_with(
                "l;ylfu",
                Some(0x20),
                detection,
                |_backspaces, _text, _vk| false,
            )
        };

        assert!(!committed);
        assert_eq!(crate::stats::snapshot(), stats_before);
        STATE.with(|state| {
            let state = state.borrow();
            assert!(state.undo.is_none());
            assert!(state.pending_hkl.is_none());
        });
    }
}
