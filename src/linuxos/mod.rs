//! The Linux (evdev + uinput + dconf) OS layer.
//!
//! RightType's Windows layer is split three ways: a low-level keyboard hook
//! (`hook.rs`), synthetic input (`inject.rs`) and layout/foreground queries
//! (`policy` KLIDs + `focus.rs`). This module is the same layer for Linux:
//!
//! | Windows | here |
//! | --- | --- |
//! | `WH_KEYBOARD_LL` hook | [`capture`] reading `/dev/input/event*` |
//! | `SendInput` | [`inject`] writing `/dev/uinput` |
//! | `GetKeyboardLayout` / `ActivateKeyboardLayout` | [`layout`] via the desktop's own switch key |
//! | `GetForegroundWindow` | not available on Wayland - see [`layout::describe`] |
//!
//! The detection core (`policy`, `detect`, `dict`, `buffer`, `segment`, ...) is
//! reused unchanged: it never calls the OS, so it compiles and passes its own
//! unit tests on Linux (`cargo test --lib`).

pub mod capture;
pub mod config;
pub mod control;
pub mod inject;
pub mod keytable;
pub mod layout;
pub mod live;
pub mod words;
