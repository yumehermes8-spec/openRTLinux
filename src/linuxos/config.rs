//! `~/.config/openrtlinux/config.toml` — the settings the daemon and the tray share.
//!
//! Written so that the tray/GUI (Python, GTK) and the daemon (Rust) never fight
//! over the file: the GUI writes it and asks the daemon to reload, the daemon
//! only ever reads it. Every field has a default, so a missing file is fine.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::policy::InputLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Fix words at a boundary, as you type.
    Auto,
    /// Only fix when asked (Shift+Backspace).
    Manual,
    /// Fix at a boundary *and* let the hotkey swap the last fix back and forth.
    Both,
    /// Watched but never touches anything.
    Off,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Manual => "manual",
            Mode::Both => "both",
            Mode::Off => "off",
        }
    }

    pub fn parse(text: &str) -> Option<Mode> {
        match text.trim().to_lowercase().as_str() {
            "auto" => Some(Mode::Auto),
            "manual" => Some(Mode::Manual),
            "both" | "auto+hotkey" | "auto+manual" => Some(Mode::Both),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// The chord the typist presses to fix the word under the cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FixChord {
    ShiftBackspace,
    ShiftSpace,
}

impl FixChord {
    /// Accepts what a person would write: `shift+backspace`, `Shift + Space`,
    /// `shift spacebar`, `space`...
    pub fn parse(text: &str) -> Option<Self> {
        let key: String = text
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .collect();
        let key = key.replace("backspace", "backspace").replace("spacebar", "space");
        if key.contains("backspace") {
            Some(FixChord::ShiftBackspace)
        } else if key.contains("space") {
            Some(FixChord::ShiftSpace)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FixChord::ShiftBackspace => "shift+backspace",
            FixChord::ShiftSpace => "shift+spacebar",
        }
    }

    /// What to show a person, and what to write in the config file.
    pub fn label(self) -> &'static str {
        match self {
            FixChord::ShiftBackspace => "Shift+Backspace",
            FixChord::ShiftSpace => "Shift+Spacebar",
        }
    }
}

/// How long a word that a boundary already closed stays reachable for the fix
/// chord, in seconds.
///
/// The daemon cannot see the pointer or the window focus, so a word remembered
/// from another document would be deleted at whatever cursor is there now.
/// 0 = never forget it (the pre-TTL behaviour).
pub const DEFAULT_RECALL_SECONDS: u64 = 15;

/// How long the newest fix can be swapped back or undone, in seconds. Longer
/// than [`DEFAULT_RECALL_SECONDS`]: a wrong fix is noticed later than a wrong
/// word. 0 = never forget it.
pub const DEFAULT_UNDO_SECONDS: u64 = 60;

/// Seconds as the milliseconds the decisions compare. 0 means "no expiry".
pub const fn window_ms(seconds: u64) -> u64 {
    if seconds == 0 {
        u64::MAX
    } else {
        seconds.saturating_mul(1000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mode: Mode,
    /// Fix decisions are made and logged, nothing is typed or switched.
    pub dry_run: bool,
    /// Command that reports the active input source.
    pub current_command: String,
    /// Command that reports the desktop's layout switch binding.
    pub switch_command: String,
    /// Start the tray when the session starts.
    pub tray_autostart: bool,
    /// Empty = detect from the desktop.
    pub layout: String,
    /// Which chord fixes the word under the cursor: `shift+backspace` (default)
    /// or `shift+space`. Stored as text so the settings window can write it.
    pub fix_word_hotkey: String,
    /// Seconds a word that a boundary already closed stays reachable for the fix
    /// chord; after that the daemon forgets it (0 = never forget).
    pub recall_seconds: u64,
    /// Seconds the newest fix can still be swapped back or undone (0 = never).
    pub undo_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::Auto,
            dry_run: true, // safe by default: a fresh install only watches
            current_command: crate::linuxos::layout::GNOME_CURRENT.to_string(),
            switch_command: crate::linuxos::layout::GNOME_SWITCH.to_string(),
            tray_autostart: false,
            layout: String::new(),
            fix_word_hotkey: "shift+backspace".to_string(),
            recall_seconds: DEFAULT_RECALL_SECONDS,
            undo_seconds: DEFAULT_UNDO_SECONDS,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            format!("{}/.config", std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        });
        Path::new(&base).join("openrtlinux").join("config.toml")
    }

    pub fn load() -> Config {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }

    /// The layout the daemon should start with, if the config names one.
    /// The configured fix chord, defaulting to Shift+Backspace.
    pub fn fix_chord(&self) -> FixChord {
        FixChord::parse(&self.fix_word_hotkey).unwrap_or(FixChord::ShiftBackspace)
    }

    pub fn layout_override(&self) -> Option<InputLayout> {
        let name = self.layout.trim().to_lowercase();
        if name.starts_with("th") {
            Some(InputLayout::ThaiKedmanee)
        } else if name.starts_with("us") || name.starts_with("en") {
            Some(InputLayout::UsQwerty)
        } else {
            None
        }
    }
}
