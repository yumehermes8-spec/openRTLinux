//! Which keyboard layout is active, and how to change it: the Linux stand-in
//! for `HKL` + `ActivateKeyboardLayout`.
//!
//! Two findings shape this file, both verified on the GNOME/Wayland box:
//!
//! 1. **Reading** the active layout: ask the desktop. GNOME keeps the most
//!    recently used sources in `org.gnome.desktop.input-sources mru-sources`,
//!    whose first entry is the active one. (Windows told us this directly; on
//!    Wayland no platform API does, so the shell's own bookkeeping is used.)
//! 2. **Switching** it: writing `org.gnome.desktop.input-sources current` does
//!    **not** change the live layout on GNOME 45+ - but pressing the desktop's
//!    own switch-key binding (read from `switch-input-source`, e.g. `grave`)
//!    through uinput does. So we press the user's own key, the same way the
//!    typist would, and then *verify* by reading again.

use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use evdev::KeyCode;

use crate::linuxos::keytable;
use crate::policy::InputLayout;

/// GNOME: the active input source is the first entry of this list.
pub const GNOME_CURRENT: &str = "gsettings get org.gnome.desktop.input-sources mru-sources";
/// GNOME: the user's own layout switch binding, e.g. `['grave']`.
pub const GNOME_SWITCH: &str = "gsettings get org.gnome.desktop.wm.keybindings switch-input-source";

fn run(command: &str) -> Option<String> {
    let out = Command::new("sh").arg("-c").arg(command).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The active layout, as far as the desktop will say.
pub fn current(command: &str) -> Option<InputLayout> {
    let text = run(command)?;
    // mru-sources looks like: [('xkb', 'th'), ('xkb', 'us')] - first entry wins.
    for part in text.split('\'') {
        match part {
            "th" => return Some(InputLayout::ThaiKedmanee),
            "us" | "gb" => return Some(InputLayout::UsQwerty),
            _ => {}
        }
    }
    None
}

/// One token of a key binding (`grave`, `<Super>space`) as an evdev code.
pub fn token_code(token: &str) -> Option<u16> {
    match token {
        "grave" => return Some(KeyCode::KEY_GRAVE.0),
        "space" => return Some(KeyCode::KEY_SPACE.0),
        "tab" => return Some(KeyCode::KEY_TAB.0),
        "caps_lock" | "capslock" => return Some(KeyCode::KEY_CAPSLOCK.0),
        "shift" | "shift_l" => return Some(KeyCode::KEY_LEFTSHIFT.0),
        "ctrl" | "control" | "control_l" => return Some(KeyCode::KEY_LEFTCTRL.0),
        "alt" | "alt_l" => return Some(KeyCode::KEY_LEFTALT.0),
        "super" | "super_l" | "meta" | "meta_l" => return Some(KeyCode::KEY_LEFTMETA.0),
        _ => {}
    }
    keytable::key_for(token.chars().next()?).map(|(code, _)| code)
}

/// The chord to press to switch layout, from the desktop's own binding.
pub fn switch_chord(command: &str) -> Vec<u16> {
    let Some(text) = run(command) else {
        return Vec::new();
    };
    let mut tokens: Vec<String> = Vec::new();
    for raw in text
        .replace(['[', ']', '\''], " ")
        .replace('<', " <")
        .split_whitespace()
    {
        let token = raw.trim().trim_start_matches('<').trim_end_matches('>');
        if !token.is_empty() {
            tokens.push(token.to_lowercase());
        }
    }
    tokens.iter().filter_map(|t| token_code(t)).collect()
}

/// Press `chord` (through the injector) and check the desktop agrees.
pub fn switch_to(
    target: InputLayout,
    chord: &[u16],
    injector: &mut crate::linuxos::inject::Injector,
    current_command: &str,
    timeout: Duration,
) -> Result<(), String> {
    if chord.is_empty() {
        return Err("no layout switch key known - set --switch-command".into());
    }
    for attempt in 0..2 {
        injector
            .chord(chord)
            .map_err(|e| format!("could not press the switch key: {e}"))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if current(current_command) == Some(target) {
                return Ok(());
            }
            sleep(Duration::from_millis(60));
        }
        let _ = attempt;
    }
    Err(format!(
        "pressed the switch key but the layout did not become {target:?}"
    ))
}

/// What this OS layer cannot do (said once, where it is decided).
pub fn describe(current_command: &str, switch_command: &str) -> String {
    let layout = current(current_command)
        .map(|l| format!("{l:?}"))
        .unwrap_or_else(|| "unknown".into());
    let chord = switch_chord(switch_command);
    let keys = if chord.is_empty() {
        "none".to_string()
    } else {
        chord
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("+")
    };
    format!("layout now: {layout} · switch chord: {keys} · focused-app rules: unavailable on Wayland")
}
