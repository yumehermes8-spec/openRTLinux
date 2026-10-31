//! Key capture from `/dev/input/event*` - the Linux stand-in for the Windows
//! low-level keyboard hook.
//!
//! The hook's job on Windows is: turn a raw virtual key into "the character the
//! active layout produced" and tell the core about modifiers. We do the same
//! from evdev codes, using [`crate::linuxos::keytable`] for the physical key and
//! the core's own layout tables for the character.
//!
//! Our own virtual keyboard is filtered by name: reading back what we inject
//! would make the engine argue with itself (a paid-for lesson from the earlier
//! Thai layout fixer on this host).

use std::collections::HashMap;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use evdev::{Device, EventType, InputEvent, KeyCode};

/// Modifier state, tracked here because evdev has no "what is held now" query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    pub caps: bool,
}

impl Mods {
    /// Is any modifier still physically down?
    ///
    /// The manual hotkey *is* a modified chord: the typist is still holding Shift
    /// when we decide to type. Anything we inject then arrives shifted - Thai
    /// turns into the upper-row glyphs, Latin into capitals - which is exactly
    /// what "the word came out with capitals and scrambled" looks like.
    pub fn any_held(self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RawKey {
    pub code: u16,
    /// 1 = press, 0 = release, 2 = repeat.
    pub value: i32,
    pub mods: Mods,
}

pub struct Capture {
    devices: Vec<(PathBuf, Device)>,
    /// Path -> inode of the device node we opened.
    ///
    /// `/dev/input/eventN` is *reused*: a tool (or a replugged keyboard) that
    /// goes away and comes back usually lands on the same path, and a stale file
    /// descriptor for it reads nothing at all. Comparing inodes is what tells
    /// "the same device" from "the same path".
    inodes: HashMap<PathBuf, u64>,
    mods: Mods,
    /// Keyboards appear while we run: a virtual one is created the moment a
    /// tool starts, and a real one can be plugged in. Re-scan instead of
    /// silently ignoring them (a lesson paid for on this host).
    last_scan: Instant,
}

/// Is this device something a person types on?
///
/// Requiring `KEY_A` alone is too strict: a *virtual* keyboard (the test tool,
/// or another remapper) may declare only the keys it sends. So: it must send
/// spaces and enters, and it must carry a real alphabet.
fn is_keyboard(dev: &Device) -> bool {
    let Some(keys) = dev.supported_keys() else {
        return false;
    };
    if !keys.contains(KeyCode::KEY_SPACE) || !keys.contains(KeyCode::KEY_ENTER) {
        return false;
    }
    let letters = crate::linuxos::keytable::codes()
        .into_iter()
        .filter(|code| keys.contains(KeyCode::new(*code)))
        .count();
    letters >= 20
}

impl Capture {
    /// Every readable keyboard. Fails with the fix in the message, not a panic.
    pub fn open() -> io::Result<Self> {
        let mut devices = Vec::new();
        let mut inodes: HashMap<PathBuf, u64> = HashMap::new();
        for (path, dev) in evdev::enumerate() {
            let name = dev.name().unwrap_or_default().to_string();
            if is_ours(&name) {
                continue; // never read our own injected keys back
            }
            if is_keyboard(&dev) {
                let _ = dev.set_nonblocking(true); // one blocking device would stall every other one
                if let Ok(meta) = std::fs::metadata(&path) {
                    inodes.insert(path.clone(), meta.ino());
                }
                devices.push((path, dev));
            }
        }
        if devices.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no readable keyboards: add this user to the input group \
                 (sudo usermod -aG input $USER) or grant a uaccess ACL on /dev/input/event*",
            ));
        }
        Ok(Capture {
            devices,
            inodes,
            mods: Mods::default(),
            last_scan: Instant::now(),
        })
    }

    pub fn describe(&self) -> String {
        let names: Vec<String> = self
            .devices
            .iter()
            .map(|(path, dev)| {
                format!("{} ({})", dev.name().unwrap_or("?"), path.display())
            })
            .collect();
        format!("{} keyboard(s): {}", self.devices.len(), names.join(", "))
    }

    pub fn mods(&self) -> Mods {
        self.mods
    }

    /// Wait (briefly) until the typist lets go of the modifiers, returning the
    /// key events that arrived meanwhile so the caller can still process them.
    ///
    /// Called before the hotkey's plan is applied: injecting while Shift is down
    /// produces capitals and Thai upper-row glyphs.
    pub fn wait_for_modifier_release(&mut self, timeout: Duration) -> Vec<RawKey> {
        let deadline = Instant::now() + timeout;
        let mut collected = Vec::new();
        while self.mods.any_held() && Instant::now() < deadline {
            for key in self.poll() {
                if key.value != 1 {
                    collected.push(key); // a release event: keep it for the caller
                } else if !is_modifier_code(key.code) {
                    collected.push(key);
                }
            }
            if self.mods.any_held() {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        collected
    }

    /// Add keyboards that showed up after we started, and pick up a path that now
    /// belongs to a *different* device (the node is reused).
    fn rescan(&mut self) {
        for (path, dev) in evdev::enumerate() {
            if is_ours(&dev.name().unwrap_or_default()) {
                continue;
            }
            let ino = std::fs::metadata(&path).map(|m| m.ino()).ok();
            let known = self
                .devices
                .iter()
                .position(|(known_path, _)| known_path == &path);
            match (known, ino) {
                (Some(index), Some(ino)) if self.inodes.get(&path) == Some(&ino) => {
                    let _ = index;
                    continue; // same device, already watched
                }
                (Some(index), _) => {
                    let _ = self.devices.remove(index); // stale fd: the path is a new device now
                }
                _ => {}
            }
            if !is_keyboard(&dev) {
                continue;
            }
            let _ = dev.set_nonblocking(true);
            if let Some(ino) = ino {
                self.inodes.insert(path.clone(), ino);
            }
            self.devices.push((path, dev));
        }
    }

    /// Whatever arrived since the last call (non-blocking).
    pub fn poll(&mut self) -> Vec<RawKey> {
        let mut out = Vec::new();
        if self.last_scan.elapsed() > Duration::from_millis(1000) {
            self.rescan();
            self.last_scan = Instant::now();
        }
        let mut mods = self.mods;
        let mut gone: Vec<PathBuf> = Vec::new();
        for (path, dev) in self.devices.iter_mut() {
            let events: Vec<InputEvent> = match dev.fetch_events() {
                Ok(iter) => iter.collect(),
                Err(err) if err.raw_os_error() == Some(19) => {
                    gone.push(path.clone()); // ENODEV: the device is gone
                    continue;
                }
                Err(_) => continue,
            };
            for ev in events {
                if ev.event_type() != EventType::KEY {
                    continue;
                }
                let (code, value) = (ev.code(), ev.value());
                track(&mut mods, code, value);
                out.push(RawKey {
                    code,
                    value,
                    mods,
                });
            }
        }
        self.mods = mods;
        for path in gone {
            self.devices.retain(|(known, _)| known != &path);
            self.inodes.remove(&path);
        }
        out
    }
}

/// Is this key code a modifier key?
pub fn is_modifier_code(code: u16) -> bool {
    [
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_RIGHTALT,
        KeyCode::KEY_LEFTMETA,
        KeyCode::KEY_RIGHTMETA,
        KeyCode::KEY_CAPSLOCK,
    ]
    .iter()
    .any(|key| key.0 == code)
}

/// Modifier bookkeeping, kept out of `&mut self` so the device borrow is short.
fn track(mods: &mut Mods, code: u16, value: i32) {
    let on = value != 0;
    if code == KeyCode::KEY_LEFTSHIFT.0 || code == KeyCode::KEY_RIGHTSHIFT.0 {
        mods.shift = on;
    } else if code == KeyCode::KEY_LEFTCTRL.0 || code == KeyCode::KEY_RIGHTCTRL.0 {
        mods.ctrl = on;
    } else if code == KeyCode::KEY_LEFTALT.0 || code == KeyCode::KEY_RIGHTALT.0 {
        mods.alt = on;
    } else if code == KeyCode::KEY_LEFTMETA.0 || code == KeyCode::KEY_RIGHTMETA.0 {
        mods.meta = on;
    } else if code == KeyCode::KEY_CAPSLOCK.0 && value == 1 {
        mods.caps = !mods.caps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_held_is_true_for_every_modifier() {
        let plain = Mods::default();
        assert!(!plain.any_held(), "nothing held means nothing to wait for");
        for mods in [
            Mods { shift: true, ..plain },
            Mods { ctrl: true, ..plain },
            Mods { alt: true, ..plain },
            Mods { meta: true, ..plain },
        ] {
            assert!(mods.any_held());
        }
        // CapsLock is not a held modifier: it is a latch, and it must not block
        let latched = Mods { caps: true, ..plain };
        assert!(!latched.any_held());
    }

    #[test]
    fn modifier_codes_are_recognised() {
        for code in [
            KeyCode::KEY_LEFTSHIFT.0,
            KeyCode::KEY_RIGHTSHIFT.0,
            KeyCode::KEY_LEFTCTRL.0,
            KeyCode::KEY_LEFTALT.0,
            KeyCode::KEY_LEFTMETA.0,
            KeyCode::KEY_CAPSLOCK.0,
        ] {
            assert!(is_modifier_code(code), "{code} should count as a modifier");
        }
        for code in [KeyCode::KEY_A.0, KeyCode::KEY_BACKSPACE.0, KeyCode::KEY_SPACE.0] {
            assert!(!is_modifier_code(code), "{code} is not a modifier");
        }
    }
}
